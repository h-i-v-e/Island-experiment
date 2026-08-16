# Sea Coast/Wave and River-Silt Mask Plan

## 1. Purpose

Add a generated two-channel texture that the Unity sea material can use during
the next sea-rendering stage.

The texture will encode two independent masks:

- **R: coast/wave mask** — `1` on all land and at sea level, fading through
  shallow water to `0` at five metres depth;
- **G: river-mouth silt mask** — a spatial plume emitted by final river mouths,
  strongest at the outlet and fading horizontally out into the sea.

This document covers generation, native export, Unity upload, validation, and
the data contract. It deliberately does not implement the final sea colour,
opacity, foam, or wave response that will consume the mask.

## 2. Confirmed Existing Behaviour

### 2.1 Original C++ implementation

The original C++ project does not export the proposed RG mask, but it contains
the relevant land/sea precedent in `CreateSeaDepthMap` in `src/unity.cpp`:

1. choose a terrain LOD from the requested texture dimension;
2. slice the terrain to the required vertical range;
3. clamp every land vertex above sea level down to zero;
4. rasterize the resulting mesh into a height map;
5. normalize and export the float buffer.

That produced a sea-depth field whose land portion was zero. It did not identify
river mouths and did not create a silt plume.

### 2.2 Current Rust implementation

The Rust island retains the final authoritative LOD0 `Terrain`, its triangle
sampling index, final rivers, the final river mesh, and material data.
`Island::surface_maps` already bakes derived textures from final terrain into
owned byte buffers and can divide large raster jobs into disjoint row ranges.

The existing `Island::sea_depth_map` is not suitable for the red-channel
contract. It normalizes against `max_height * 0.28`, whereas the coast/wave mask
needs a fixed physical five-metre interval and must remain fully white over
land.

Final rivers already have the information needed to define mouths:

- a main river has `join == None`;
- the final `RiverNetwork` retains the flood-filled, corner-connected ocean
  classification while rivers are being finalized;
- a river node retains its final position, surface, and accumulated flow;
- final traced rivers are required to end in the connected ocean, rather than a
  disconnected below-sea-level basin.

### 2.3 Current Unity implementation

Unity generates the island on a background task, copies native surface-map
buffers into managed byte arrays, releases the native export, and creates the
Unity textures on the main thread. The existing surface textures are 2048 by
2048, linear, clamped, trilinearly filtered, and have generated mipmaps.

The Unity island is 2,000 metres across. Rust terrain coordinates are normalized
to `[0, 1]`, so five metres vertically is `5 / 2000 = 0.0025` in the generated
mesh coordinate system. The implementation must use the common world-size
constant rather than deriving this from `max_height`.

## 3. Texture Contract

Use one interleaved, row-major, two-channel unsigned-byte buffer.

| Property | Contract |
| --- | --- |
| Logical name | Sea coast-wave/silt mask |
| Native layout | `RG`, one `u8` per channel, two bytes per texel |
| Unity format | `TextureFormat.RG16` (8-bit R plus 8-bit G) |
| Colour space | Linear; never sRGB-corrected |
| Dimensions | 2048 by 2048 initially, matching the shared surface maps |
| UV mapping | U uses normalized terrain X; V uses normalized terrain Y |
| Row order | Exactly the same orientation as current Rust surface maps |
| Wrapping | Clamp |
| Filtering | Trilinear with generated mipmaps |

### 3.1 Red channel: land plus the shallow-water wave blend

At full-resolution mip zero:

```text
depth_metres = max(-terrain_height_normalized * island_world_metres, 0)
coast_wave_weight = saturate(1 - depth_metres / 5)
R = round(255 * coast_wave_weight)
```

This produces the required endpoints:

- every land texel is `255` because its underwater depth is zero;
- a sea-level texel is `255`;
- a texel 2.5 metres below sea level is approximately `128`;
- a texel five metres below sea level or deeper is `0`.

The red channel is therefore both a complete land mask and a physically scaled
shallow-water transition for blending waves near the coast. It is not a hard
land/sea classification.

Bilinear filtering and mipmaps provide sub-texel transition filtering to the
consuming shader. At 2048 resolution, one texel spans approximately 0.98 metres
across a 2 km island, so an additional CPU-side coastline blur is unnecessary.

The initial implementation should sample texel centres using the final LOD0
triangle index. A later optional 2x2 coverage mode may be considered only if
coastline aliasing remains visible; it should not be part of the first pass
because it would quadruple terrain samples.

### 3.2 Green channel: horizontal river-mouth silt

The green channel is independent of the five-metre red-channel depth ramp. It is
zero above sea level and away from river-mouth influence. At and below sea
level it contains a directional plume that is strongest at the mouth and fades
outward into the sea:

```text
G = round(255 * mouth_influence)
```

The mouth influence is a two-dimensional world-space falloff based on distance
along and across the river's final downstream direction. It may continue into
water deeper than five metres until its horizontal plume reaches zero; seabed
depth does not implicitly shorten or reshape it. A later sea shader can combine
R and G if it wants depth-aware silt rendering, without baking that policy into
the source data.

At exactly sea level both R and G may be `255` near a mouth. The channels are
not mutually exclusive. G is forced to zero only where final terrain is above
the sea plane, preventing the plume from painting inland terrain.

If multiple plumes overlap, combine them with `max`, not addition. This prevents
two nearby small mouths from saturating to a stronger result than the largest
river and keeps the output deterministic and bounded.

## 4. River-Mouth Definition

### 4.1 Capture mouths from the authoritative final network

Add an internal compact value type, for example:

```rust
struct RiverMouth {
    position: Vec2,
    downstream: Vec2,
    flow: u32,
}
```

Extract it while `RiverNetwork` still owns the flood-filled `ocean` vector,
before `into_parts` consumes the network. A qualifying mouth must satisfy all
of the following:

1. the river is a main outlet (`join == None`);
2. its terminal node is marked by the connected-ocean mask;
3. it has at least two distinct node positions so a downstream tangent can be
   calculated;
4. the tangent from the penultimate distinct node to the terminal node is
   finite and non-zero.

Use the terminal node's final accumulated flow. Calculate the downstream vector
from the last distinct river segment and normalize it. If several terminal
nodes occupy essentially the same mouth, retain the highest-flow candidate or
merge them deterministically.

Do not infer a mouth solely from `z <= 0`. That would reintroduce the former bug
where disconnected below-sea basins could be mistaken for the ocean.

### 4.2 Lifetime and ownership

Store the compact mouth list on `Island`. This avoids retaining a second
per-vertex ocean mask and avoids re-running flow routing when a texture is
requested. Mouth count is expected to be small, so this is negligible compared
with the terrain mesh.

If the island serialization format is still expected to round-trip all derived
assets, either:

- serialize the mouth records in a bumped format version; or
- deterministically reconstruct them on load from stored final rivers plus a
  freshly calculated connected-ocean mask.

The preferred option is serialization because it preserves the exact final
network result and makes mask export after load inexpensive. Older saves should
load with an empty mouth list or use the documented reconstruction path rather
than interpreting arbitrary submerged endpoints as mouths.

## 5. Proposed Mouth-Influence Field

The red-channel depth ramp and green-channel horizontal plume are deliberately
separate. The following horizontal plume is the recommended first
implementation and should be isolated behind named constants so the later sea
work can tune it without changing the export contract.

### 5.1 Flow normalization

Calculate once for the mask bake:

```text
flow_scale = sqrt(mouth.flow / maximum_mouth_flow)
```

The square root prevents one exceptionally large river from overwhelming all
smaller outlets.

### 5.2 Directional plume footprint

For each mouth, define a rounded, downstream-oriented plume in world metres.
Recommended initial ranges are:

```text
downstream length:  40 m at minimum flow to 200 m at maximum flow
half-width:          8 m at minimum flow to  50 m at maximum flow
upstream overlap: one half-width
```

For a sea texel, project the mouth-to-texel vector onto the mouth's downstream
and cross-stream axes. Use smoothstep falloffs longitudinally and laterally,
with a rounded cap. The short upstream overlap closes the sub-texel gap between
the final river mesh and the sea plume; it must not produce a long plume inland,
and the above-sea test forces G back to zero over land.

This directional footprint is preferable to a circular blur because it uses
the final river direction and naturally carries silt out from the outlet rather
than equally along the coastline.

### 5.3 Spatial lookup

Do not test every texel against every mouth if the number of outlets grows.
Build a small uniform 2D bin index once per bake. Insert each mouth into the bins
overlapped by its maximum plume bounds. Each raster worker then examines only
the candidates in the texel's bin.

The index may own compact `usize` mouth indices. Per-texel code must not allocate,
clone mouth records, construct temporary vectors, or lock shared state.

### 5.4 Combination order

For each texel:

1. sample the final terrain height once;
2. calculate the 0–5 m coast/wave weight and write R;
3. if terrain is above sea level, write G as zero and stop;
4. query the texel's mouth bin;
5. if the bin has no candidates, write G as zero and stop;
6. take the maximum directional mouth influence;
7. quantize that influence directly to G.

This ordering avoids plume work for all land and for sea bins outside every
plume. Deep water is not rejected merely because of its depth: horizontal plume
extent, rather than the red-channel wave ramp, determines whether it contains
silt.

## 6. Rust API and Raster Implementation

### 6.1 Owned result type

Add a public derived-map type next to `SurfaceMaps`, for example:

```rust
pub struct SeaMask {
    width: u32,
    height: u32,
    rg: Vec<u8>,
}
```

Expose read-only accessors for width, height, and `&[u8]`. The one owned
interleaved allocation is justified because it crosses the native ABI and must
remain stable until Unity copies it.

Add:

```rust
impl Island {
    pub fn sea_mask(&self, width: u32, height: u32) -> SeaMask;
}
```

The implementation should borrow `&Terrain` and `&[RiverMouth]`. It must not
clone the terrain, rivers, or triangle index.

### 6.2 Physical scale

Move the existing 2,000-metre assumption used by river emitters into one Rust
constant shared by all physically scaled derived products, for example
`ISLAND_WORLD_METRES`. Use that constant for the five-metre conversion and add a
test that guards the expected normalized depth of `0.0025`.

Do not use `IslandOptions::max_height` for this conversion. `max_height` changes
mountain amplitude, not the Unity metres represented by one normalized Z unit.

### 6.3 Parallel execution

Follow the existing `bake_surface_maps` strategy:

- allocate `width * height * 2` bytes once;
- split the output into non-overlapping whole-row chunks;
- let scoped worker threads borrow terrain, mouths, and the read-only spatial
  index;
- write directly into disjoint mutable row slices;
- use a serial path for small test images where thread setup costs more than it
  saves.

The loop should calculate normalized U/V and world X/Y incrementally where that
improves clarity and removes repeated divisions. Any fast path must preserve the
same endpoints and orientation as the current surface maps.

### 6.4 Failure and size handling

Match the current derived-map conventions: clamp width and height to at least
one at the FFI entry point and use checked length arithmetic before allocating.
An allocation-size overflow should return an empty/default native export rather
than expose a mismatched pointer and dimensions.

The existing Rust option validation is unrelated to this derived map and should
not be expanded.

## 7. C ABI

Add a dedicated export rather than packing this buffer into
`ExportSurfaceMaps`. The sea mask has a different consumer and may evolve on a
different schedule.

Recommended layout:

```c
typedef struct ExportSeaMask {
    void *handle;
    int32_t width;
    int32_t height;
    const uint8_t *rg;
} ExportSeaMask;
```

Recommended functions:

```c
void CreateSeaMask(
    const void *island,
    int32_t dimension,
    ExportSeaMask *output);

void ReleaseSeaMask(ExportSeaMask *output);
```

Implementation ownership rules:

1. box one `SeaMask` in `CreateSeaMask`;
2. expose the stable `Vec<u8>` pointer while the box is alive;
3. store the box pointer in `handle`;
4. reconstruct and drop it exactly once in `ReleaseSeaMask`;
5. reset every output field to zero/null after release;
6. return a fully default export for a null island or invalid output.

Update both the Rust C header and `MotuNative.cs` together. Keep the C# struct
field order and pointer widths identical to Rust's `#[repr(C)]` definition.

## 8. Unity Background Preparation

### 8.1 Managed prepared data

Add `PreparedSeaMask` containing:

- dimension;
- one `byte[] rg` of exact length `dimension * dimension * 2`.

Add it to `PreparedIsland`. During `PrepareIsland`, after native island
generation and alongside the existing surface-map preparation:

1. call `CreateSeaMask` on the worker thread;
2. validate non-null handle/pointer and exact dimensions;
3. allocate the exact managed byte array;
4. `Marshal.Copy` exactly `pixelCount * 2` bytes;
5. release the native export in `finally` even on cancellation or copy failure;
6. check cancellation before beginning the following expensive preparation
   stage.

Do not call Unity texture APIs from this worker phase.

### 8.2 Main-thread texture creation

Add an owned `Texture2D seaMaskTexture` field to `IslandViewer`. On the main
thread:

```csharp
new Texture2D(dimension, dimension, TextureFormat.RG16, true, true)
```

Then:

- use `SetPixelData(rg, 0)`, not `LoadRawTextureData` against an allocated mip
  chain;
- call `Apply(true, true)` to generate mipmaps and make the CPU copy unreadable;
- use `TextureWrapMode.Clamp`;
- use `FilterMode.Trilinear` and the same anisotropy as the surface maps;
- bind it to `_SeaMask` on `seaMaterial`;
- destroy the previous texture during regeneration and in `OnDestroy`.

The river material should not consume the map in this phase. Its current
height/depth-based estuary behaviour remains unchanged until the upcoming sea
shader work deliberately replaces or shares it.

### 8.3 Shader contract stub and diagnostics

Add `_SeaMask` as a no-scale/no-offset linear texture property on the sea shader
only when Unity binding is implemented. Do not change visible water output yet.

For validation, add a temporary editor/debug mode or a small diagnostic shader
keyword that can display:

- R as white land plus a grayscale shallow-water band fading to black by 5 m;
- G as the silt plume;
- RG as a false-colour overlay.

Keep the diagnostic disabled by default and remove any temporary GUI control
after visual acceptance unless it proves useful for later sea tuning.

## 9. Tests

### 9.1 Rust unit tests

Add small synthetic-mesh tests that do not require full island generation:

1. terrain above sea level produces `R=255, G=0`;
2. terrain exactly at sea level produces `R=255`;
3. terrain at 2.5 m depth produces R approximately `128` within byte
   quantization error;
4. terrain at 5 m depth and deeper produces `R=0`;
5. at a qualifying mouth and sea level, the unquantized G weight is 1;
6. G decreases smoothly along and across the plume and reaches zero at its
   configured horizontal bounds;
7. a plume may remain non-zero in water deeper than five metres while R is zero;
8. sea outside every plume has G zero regardless of depth;
9. above-sea terrain within a plume footprint still has G zero;
10. an outlet into a disconnected submerged basin does not create a mouth;
11. a tributary with `join != None` does not create a second mouth;
12. overlapping plumes use max and cannot overflow;
13. repeated bakes are byte-for-byte deterministic;
14. serial and threaded paths produce identical buffers;
15. output length is exactly `width * height * 2`.

Factor the red-channel depth function and green-channel directional plume
function into separate small pure helpers so each can be tested without a 2048
texture and neither accidentally acquires the other's falloff.

### 9.2 FFI tests

Extend native-boundary tests to verify:

- null input leaves a default export;
- valid creation returns a non-null handle and data pointer;
- dimensions match the request;
- the accessible byte length implied by the contract is exactly `w * h * 2`;
- release nulls every field;
- a second independent create/release works, catching stale global ownership.

### 9.3 Unity validation

Validate at least three deterministic seeds, including:

- one broad river mouth on a shallow shelf;
- several nearby river mouths;
- a steep outlet that reaches five metres depth quickly.

For each seed inspect the false-colour diagnostic and confirm:

- every visible land area is represented in R;
- R remains full strength at the shoreline and fades smoothly to zero between
  sea level and 5 m depth;
- no disconnected underwater basin receives a river-mouth plume;
- G joins the final main river outlet without a gap;
- G fades horizontally along and across the outgoing plume rather than changing
  merely because seabed depth changes;
- no plume crosses over a headland merely because it is spatially nearby;
- mipmapped coastlines remain stable as the camera moves;
- regenerating or exiting play mode does not leak a texture or native export.

## 10. Performance and Instrumentation

Add a named timing span such as `sea_mask.bake` around Rust generation and log
the Unity preparation/upload durations separately while developing.

At 2048 resolution the final buffer is 8 MiB:

```text
2048 * 2048 * 2 = 8,388,608 bytes
```

The expected transient ownership is one 8 MiB Rust allocation plus one 8 MiB
managed copy until the native export is released. Unity then uploads its own GPU
texture and discards the readable CPU texture copy after `Apply`.

Acceptance targets:

- no per-texel heap allocation;
- no terrain, river, or mouth-list clone during baking;
- no lock in the raster inner loop;
- no more than one owned Rust pixel buffer and one managed transfer buffer;
- mask generation remains on the existing background generation task;
- report mask bake time separately so a regression is visible.

Do not optimize by reducing resolution before measuring. If baking is material
relative to total generation, first profile terrain sampling and spatial-bin
queries, then consider combining height sampling with another 2048 derived-map
pass in a later change.

## 11. Implementation Phases

### Phase 1 — Lock the contract and add pure helpers

- Add the common 2 km world-scale constant.
- Add separate pure coast/wave-depth and directional-plume helpers.
- Add endpoint and overlap tests.
- Document channel order and texture orientation beside the result type.

**Exit criterion:** the R values at 0 m, 2.5 m, and 5 m are exact within RG8
quantization, while the independent G plume field is deterministic and has no
implicit depth multiplier.

### Phase 2 — Preserve final river-mouth metadata

- Add `RiverMouth`.
- Extract only connected-ocean main outlets before consuming `RiverNetwork`.
- Preserve final position, downstream tangent, and flow.
- Store the compact list on `Island` and settle save/load behaviour.
- Add river-network tests for tributaries and disconnected basins.

**Exit criterion:** every retained mouth corresponds to one final connected-sea
outlet and no inland low point qualifies.

### Phase 3 — Bake the Rust RG mask

- Add `SeaMask` and `Island::sea_mask`.
- Add the mouth spatial bins.
- Implement one-sample-per-texel coast/wave weighting and plume calculation.
- Add deterministic serial/threaded tests and timing.

**Exit criterion:** the buffer contract, channel values, orientation, and length
are covered by automated tests.

### Phase 4 — Add the native export

- Add `ExportSeaMask`, `CreateSeaMask`, and `ReleaseSeaMask`.
- Update the public C header and C# declarations.
- Add ownership/null/reset FFI tests.

**Exit criterion:** repeated create/copy/release cycles pass without leaks,
overreads, or stale pointers.

### Phase 5 — Integrate Unity preparation and upload

- Add `PreparedSeaMask` to background generation.
- Copy exactly two bytes per pixel and release native ownership in `finally`.
- Create a linear `RG16` texture on the main thread.
- Bind `_SeaMask` to the sea material and implement full cleanup.
- Add the temporary false-colour diagnostic.

**Exit criterion:** entering play mode, regenerating, cancelling generation, and
leaving play mode all work without freezing, pink materials, overread errors, or
leaked native handles.

### Phase 6 — Visual acceptance and tuning lock

- Inspect the required fixed seeds.
- Adjust only named horizontal plume constants.
- Keep the required R-channel 0–5 m depth ramp unchanged.
- Record the selected plume length/width defaults near their tests.
- Disable the debug display and leave the texture ready for the sea shader stage.

**Exit criterion:** R covers all land and fades to zero by 5 m water depth, G
originates only at real river mouths and fades horizontally into the sea, and
the next sea work can sample both independent masks without further native/API
changes.

## 12. Full Verification Commands

From `island-rs` after implementation:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Then copy the newly built native library into the Unity plugin location using
the project's established plugin-reload procedure, allow Unity to reimport it,
and run the fixed-seed visual matrix. Rust tests and a successful library build
do not replace Unity runtime validation of layout, orientation, mipmaps, and
resource lifetime.

## 13. Final Acceptance Criteria

The work is complete when all of the following are true:

1. one linear 2048 RG8-equivalent texture is exported for each generated island;
2. R is 1 for every final land texel and at sea level, approximately 0.5 at 2.5
   m water depth, and 0 at 5 m depth or deeper;
3. G is non-zero only at or below sea level within the horizontal influence of
   a connected-ocean main river mouth;
4. G is strongest at the mouth and blends outward along and across the plume,
   without inheriting R's five-metre depth falloff;
5. mouth size responds smoothly to final accumulated flow and follows the final
   downstream direction;
6. disconnected submerged basins and tributary joins do not seed silt;
7. the mask is generated without per-pixel allocation or UI-thread work;
8. Rust, C, and C# agree on field order, dimensions, byte count, and ownership;
9. Unity binds the mask to the sea material, cleans it up on regeneration, and
   has no raw-data overread or pink-shader failure;
10. automated Rust/FFI checks and the fixed-seed Unity visual matrix pass.

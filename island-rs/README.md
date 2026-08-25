# island-rs

An idiomatic Rust conversion of the Motu procedural-island project. It retains
the original free-form terrain model: random XY seed points are triangulated
with Bowyer-Watson Delaunay, and finer LODs are produced by shared-edge triangle
tessellation. It is a self-contained crate with no third-party runtime or build
service. The primary CPU implementation does not use a graphics API; the
default `gpu-generation` Cargo feature includes `wgpu` for native compute and
can be omitted from CPU-only builds.

The generator provides:

- deterministic seeded free-form terrain with configurable water coverage and elevation;
- an explicitly selected GPU generation method for hydraulic erosion and
  settled rocks, while retaining the established CPU rivers and waterfalls;
- staged perimeter-preserving XYZ smoothing between LOD refinement passes;
- graph-based thermal and hydraulic erosion over compact CSR mesh adjacency;
- persistent unconsolidated cover and noise-aligned bedrock hardness shared by
  hydraulic, thermal, and river-valley erosion;
- topology reuse across smoothing, erosion, and river passes at each LOD;
- conforming land/relief tessellation with coarse seabed preservation;
- sink-safe drainage routing with tributary joins and per-node flow accumulation;
- river jiggle/corridor smoothing, staged LOD carving, bank incision, graded
  estuaries, tributary sediment transfer, raised alluvial valleys, and connected
  shallow-water delta fans, followed by a final carve-only river pass;
- flow-width river strips with flat reaches, gradient-adaptive waterfall
  curtains, monotonic confluences, and UVs;
- high-detail-to-coarse-LOD normal baking and directional ambient occlusion textures;
- terrain meshes at three levels of detail, normals, one-pass geometrically
  clipped grid slicing, coarser-LOD edge clamping, height maps, and per-export
  hardness/forced-rock, loose-cover, and river-bed vertex attributes;
- one XY-safe support surface shared by simulation, collision, and LOD 0
  rendering, with geometrically clipped tile slicing and LOD edge clamping;
- tree, bush, and rock placement plus packed foliage, sea-depth maps, and a
  final-terrain RG sea mask carrying coastal wave depth and distance from land;
- flat spatial triangle indexing, parallel RGB rendering, and a built-in PNG encoder;
- save/load of reproducible generator inputs;
- a `cdylib` exposing the principal Unity-facing C ABI allocation/release pairs,
  including owned fixed-grid mesh batches for streamed Unity terrain.

## Build and run

```sh
cargo run --release -- --seed 666 --output test.png
```

Useful options:

```text
--width <PX> --height <PX> --seed-points <16..4096>
--water-ratio <FLOAT> --max-height <FLOAT>
--hydraulic-erosion-strength <0..8>
--hydraulic-deposition-strength <0..4>
--hydraulic-deposition-slope <1..45>
--river-source-catchment-hectares <HECTARES>
--river-source-steep-multiplier <FLOAT>
--river-source-elevation-boost <FACTOR>
```

## Generation methods

`Island::generate` and `Island::generate_with_forest` always use the primary CPU
implementation. Normal builds include the GPU code, but applications still
select it explicitly when they want the experimental path:

```toml
island-rs = { path = "../island-rs" }
```

```rust
use motu::{GenerationMethod, Island, IslandOptions};

let island = Island::generate_with_method(
    666,
    IslandOptions::default(),
    GenerationMethod::Gpu,
)
.expect("GPU generation failed");
```

Use `--no-default-features` for a CPU-only build without the `wgpu`, `pollster`,
or `bytemuck` dependencies.

The GPU implementation accelerates particle erosion and spatial rock settling,
targeting a similar natural character rather than identical terrain. After
erosion it deliberately uses the established CPU drainage, channel carving,
river-mesh and waterfall pipeline. GPU adapter or execution failures are
returned through the existing generation `Result`.

Generated islands retain their method. Version 18 saves persist that method so
loading regenerates through the same implementation; older saves load as CPU.
A CPU-only build returns an error when asked to load a GPU save. The Unity C ABI
keeps its historical CPU entry points and also exposes a method-aware entry
point for Unity's serialized CPU/GPU selector. Both viewers include the feature
in normal builds.

The Unity viewer constrains water ratio to `0.60..0.95`, river-source catchment
to `0.01..10` hectares, the steep-slope multiplier to `1..8`, and the
elevation boost to `0..20`.

## Engine-neutral procedural textures

The `island-texture-baker` binary generates deterministic albedo, height,
normal, occlusion and optional packed-mask maps from the current JSON recipes
in `texture-recipes/`. Rust owns the recipe shape and evaluator; Unity and
other engines can edit the same document without reimplementing noise or
material logic. The root `material` and `albedo` blocks define the base pass,
while the ordered `layers` stack routes each scalar source independently to
height and/or albedo through remapping, masks and output bindings.

Bake a committed recipe from the repository root:

```sh
cargo run --release \
  --manifest-path island-rs/Cargo.toml \
  --bin island-texture-baker -- \
  --recipe island-rs/texture-recipes/cracked-stone.json \
  --output island-unity/Assets/Generated/Textures/CrackedStone \
  --profile motu_unity_terrain
```

The editor protocol uses the same executable for machine-readable schema,
validation and resolution-limited previews. Preview output belongs under a
temporary directory such as `Library/ProceduralMaterialPreview/`; only an
explicit bake writes under Unity's `Assets/Generated/Textures`.

Hydraulic erosion strength is a multiplier over the generator's staged erosion
profile. The default is `1`; use `0` to disable hydraulic erosion while keeping
thermal erosion and river carving enabled. Through 45 degrees material retreats
along the live mesh surface normal. Above 45 degrees the movement direction
blends smoothly toward vertical lowering, becoming almost entirely downward as
the surface approaches 90 degrees; this prevents resistant cliff ridges being
drawn into extremely thin horizontal flanges. The erosion amount still uses
`sin(2θ)`, calculated from the vertical and horizontal components of the surface
normal: zero on level ground, maximum at 45 degrees, and smoothly back to zero
at vertical. Overhanging faces also receive no hydraulic retreat. Every inward
movement is limited relative to local edge length and incident projected
triangle area to prevent near-vertical faces collapsing into spikes. Each
hydraulic stage caches its starting signed XY face areas and their live values.
Every lateral erosion move is capped analytically against only the moved
vertex's incident faces, preserving their stage-start orientation and at least
20 percent of their original projected area without per-move allocation.

Hydraulic bedrock resistance uses the same continental/detail noise field as
the initial terrain. That material identity is sampled once on the base mesh
and propagated through adaptive tessellation, so coherent hard ridges survive
while soft basins retreat more quickly. Deposited, thermal, delta, and
alluvial material shares a persistent unconsolidated-cover account and is
removed at the full soft-material rate before the underlying bedrock. Hydraulic
deposition tracks the sediment actually removed from the terrain. Its strength
controls how quickly excess sediment settles, while the slope angle controls
where deposition fades to zero. The defaults deposit fully below 4 degrees,
taper smoothly to zero at 12 degrees, and retain sediment on steeper slopes
until the flow reaches gentler ground.

LOD 0 exports the corrected support surface directly, retaining hydraulic
erosion, adaptive terrain tessellation, rivers, and waterfalls
without a duplicate display mesh. Tile boundaries facing a coarser LOD still
morph back only on the requested side.

Hydraulic erosion uses the sequential path model because each path needs to
observe the terrain mutations made by earlier paths to form coherent drainage
and ridges. The bounded mesh-flow experiment is retained only for investigation
and can be selected with `MOTU_EXPERIMENTAL_MESH_FLOW=1`.

Terrain exports include explicit support-anchored UVs. `CreateSupportMesh`
provides an unambiguous XY-safe collider surface. `CreateMesh` and
`CreateMeshGrid` now export that same surface for LOD 0. Before final LOD
correction, LOD 1 and LOD 2 are each tessellated once more. Their pre-existing
shared vertices retain the exact final LOD 0 positions, while each inserted
edge midpoint samples its elevation from the final LOD 0 surface. This reduces
each adjacent density step without duplicating LOD 0 geometry. Each sliced
terrain export also owns a
parallel `material` array sampled from the final LOD 0 material field after
clipping: ordinary terrain uses X for normalized bedrock hardness and Y for
normalized loose cover. Z is a cached sea-proximity strength computed over LOD
0 mesh edges before final river tracing and carving: connected-sea vertices are
one through the first two world metres, then fade linearly to zero at twenty
world metres. River-bed and
sharp-terrain vertices force X to one and Y to zero while retaining this Z
proximity. Sampling after slicing keeps the attributes paired with reordered
and newly inserted boundary vertices.
All terrain and support exports are also clipped against a horizontal plane
five metres below sea level. Faces crossing the plane receive shared
interpolated boundary vertices, while deeper faces and now-unused vertices are
discarded. The authoritative full terrain remains available for height maps,
sea-depth masks, river processing, and deterministic saves.

`CreateTerrainColliderHeightMap` samples the authoritative final LOD 0 surface
on one global lattice for Unity collision. It accepts 33, 65, or 129 samples
per logical LOD 1 tile and returns respectively 2049, 4097, or 8193 samples per
world edge. Adjacent Unity tiles therefore copy their boundary rows and columns
from identical source indices. The owned export must be released with
`ReleaseTerrainColliderHeightMap`; sampling is row-parallel while output order
remains deterministic. This collision representation is a heightfield, so it
cannot reproduce overhangs, vertical faces, or multiple elevations at one XY
coordinate.

River-source selection uses one absolute catchment area for every routing pass.
Projected control areas are converted to square metres and accumulated
downstream alongside the existing vertex flow, so progressively denser meshes
measure the same physical drainage area without separate LOD controls. The
local required area rises smoothly with the selected downhill edge's steepness;
the default multiplier is four near a vertical edge. Lower catchment areas
select more sources, while higher values produce fewer. The default is `0.05`
hectares (500 square metres) at the configured maximum elevation. There is no
hard minimum source elevation. Instead, the elevation boost raises the required
catchment continuously toward sea level. Its default of nine makes the
sea-level requirement ten times the high-elevation requirement, suppressing
short coastal rivers while retaining mountain sources.
River excavation consumes loose cover
before hardness-weighted bedrock, transfers area-weighted sediment volumes
through tributaries, records alluvial/delta/shelf raises as loose cover, and
exports the unused balance from the final carve-only outlets.

`CreateRiverEmitters` derives sparse rough-water locations directly from the
final authoritative, unsliced river mesh. It measures the dihedral angle between
the two faces sharing each mesh edge and assigns the maximum incident edge
sharpness to its vertices. Coplanar triangles on the vertical waterfall sheet
therefore do not qualify, while the changes between a flat reach and the falling
sheet select the top and bottom waterfall lips. A candidate pair must also span
a flatter face no steeper than 35 degrees and a steeper face of at least 55
degrees, preventing ordinary triangulation bends within one slope class from
becoming spray. Sharp perimeter vertices remain
eligible through their other shared incident edges, retaining noisy constricted
river features. Deterministic three-dimensional spacing suppression follows.
Each compact export record contains position, normalized final vertex normal as
the outflow direction, and normalized excess sharpness. The returned vector is
owned by its opaque handle and must be released exactly once with
`ReleaseRiverEmitters`; the island does not retain a duplicate candidate array.

The default `--seed-points 1024` matches the original generator. Staged uniform,
land-only, and relief-selective passes produce roughly 500,000 irregular
vertices for a typical finest LOD while leaving underwater regions coarser.
Raster grids are created only as derived height, normal, foliage, sea-depth, or
PNG outputs; they do not define terrain connectivity.

Build the dynamic library with `cargo build --release`. The result is named
`motu` (`libmotu.dylib`, `libmotu.so`, or `motu.dll`, depending on platform).

## Validation

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

The original checkout does not currently provide a reproducible comparison
build: its checked-in C++ includes a missing `erosian_map.h`, and its root
CMake file does not add the `island/main.cpp` executable. This port therefore
preserves the product capabilities and C boundary shape rather than promising
byte-identical meshes or images from the legacy implementation.

`CreateNormalMap3DC` remains a compatibility stub returning null, matching the
disabled implementation in the C++ source. Tree-billboard batching retains the
old eight-octant C allocation/release shape and is also available through the
safe Rust mesh/decoration APIs.

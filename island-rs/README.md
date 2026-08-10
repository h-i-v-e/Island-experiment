# island-rs

An idiomatic Rust conversion of the Motu procedural-island project. It retains
the original free-form terrain model: random XY seed points are triangulated
with Bowyer-Watson Delaunay, and finer LODs are produced by shared-edge triangle
tessellation. It is a self-contained crate with no third-party runtime or build
dependencies beyond `glam`, which provides the public and internal `Vec2` and
`Vec3` math types.

The generator provides:

- deterministic seeded free-form terrain with configurable water coverage and elevation;
- staged perimeter-preserving XYZ smoothing between LOD refinement passes;
- graph-based thermal and hydraulic erosion over compact CSR mesh adjacency;
- topology reuse across smoothing, erosion, and river passes at each LOD;
- conforming land/coast/relief tessellation with coarse seabed preservation;
- mesh-native coastal evolution with noise-aligned rock hardness, directional
  wave fetch, headland/bay retreat, wave-cut platforms, conservative longshore
  sediment transport, and sheltered equilibrium beaches;
- sink-safe drainage routing with tributary joins and per-node flow accumulation;
- river jiggle/corridor smoothing, staged LOD carving, bank incision, graded
  estuaries, tributary sediment transfer, raised alluvial valleys, and connected
  shallow-water delta fans, followed by a final carve-only river pass;
- flow-width river strips with flat reaches, gradient-adaptive waterfall
  curtains, monotonic confluences, and UVs;
- high-detail-to-coarse-LOD normal baking and directional ambient occlusion textures;
- terrain meshes at three levels of detail, normals, one-pass geometrically
  clipped grid slicing, coarser-LOD edge clamping, and height maps;
- tree, bush, and rock placement plus packed foliage and sea-depth maps;
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
--coastal-erosion-strength <FLOAT> --beach-formation-strength <FLOAT>
--hydraulic-erosion-strength <0..8>
--hydraulic-deposition-strength <0..4>
--hydraulic-deposition-slope <1..45>
--river-lod2-threshold <SD> --river-lod1-threshold <SD>
--river-broad-threshold <SD> --river-land-threshold <SD>
--river-final-threshold <SD>
```

The Unity viewer constrains water ratio to `0.60..0.95` and each river-source
threshold to `0..16`; its coastal controls use `0..4`. The Rust API and CLI do
not enforce those viewer ranges.

Coastal erosion follows the actual triangle/sea-level contour. It traces wave
fetch through face adjacency and erodes coherent softer geology derived from
the same continental/detail noise that creates the initial terrain. Soft rock
retreats through a broad vertical band, while hard rock receives a narrow,
stronger toe attack and very little upper-face erosion, retaining steep
headlands rather than smoothing them into gentle slopes. Beach formation
redistributes only the removed volume and excludes resistant coast from broad
profiles. Gentle near-shore sediment is tracked as unconsolidated cover rather
than inheriting the hardness of the rock beneath it; exposed cover is removed
rapidly before bedrock erosion begins, while sheltered cover can remain as a
beach. Both controls default to `1`; setting coastal erosion to `0` bypasses the
complete coastal stage, including its selective tessellation.

Hydraulic erosion strength is a multiplier over the generator's staged erosion
profile. The default is `1`; use `0` to disable hydraulic erosion while keeping
thermal erosion and river carving enabled.

Hydraulic deposition tracks the sediment actually removed from the terrain.
Its strength controls how quickly excess sediment settles, while the slope
angle controls where deposition fades to zero. The defaults deposit fully
below 4 degrees, taper smoothly to zero at 12 degrees, and retain sediment on
steeper slopes until the flow reaches gentler ground.

River thresholds are standard deviations above mean accumulated flow. Lower
values select more river sources; higher values produce fewer rivers. The five
controls correspond to the successive coarse, medium, broad, land-refined, and
final-detail routing/carving passes.

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

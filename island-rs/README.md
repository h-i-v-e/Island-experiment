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
- persistent unconsolidated cover and noise-aligned bedrock hardness shared by
  hydraulic, thermal, coastal, and river-valley erosion;
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
  clipped grid slicing, coarser-LOD edge clamping, height maps, and per-export
  hardness, loose-cover, and river-bed vertex attributes;
- one XY-safe support surface shared by simulation, collision, and LOD 0
  rendering, with geometrically clipped tile slicing and LOD edge clamping;
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
--cliff-render-strength <FLOAT>
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
while soft basins retreat more quickly. Deposited, thermal, beach, delta, and
alluvial material shares a persistent unconsolidated-cover account and is
removed at the full soft-material rate before the underlying bedrock. Hydraulic
deposition tracks the sediment actually removed from the terrain. Its strength
controls how quickly excess sediment settles, while the slope angle controls
where deposition fades to zero. The defaults deposit fully below 4 degrees,
taper smoothly to zero at 12 degrees, and retain sediment on steeper slopes
until the flow reaches gentler ground.

LOD 0 exports the corrected support surface directly, retaining hydraulic
erosion, coastal erosion, adaptive terrain tessellation, rivers, and waterfalls
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
clipping: X is bedrock hardness, Y is normalized loose cover, and Z is final
river-bed coverage. Sampling after slicing keeps the attributes paired with
reordered and newly inserted boundary vertices.

River thresholds are standard deviations above mean accumulated flow. Lower
values select more river sources; higher values produce fewer rivers. The five
controls correspond to the successive coarse, medium, broad, land-refined, and
final-detail routing/carving passes. River excavation consumes loose cover
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

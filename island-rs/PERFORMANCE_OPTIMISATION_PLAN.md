# Island Generator Performance Optimisation Plan

## Status

Implementation started on 2026-08-12. Phases 0-2, the main tessellation
improvements from Phase 3, and parallel surface noise/smoothing/thermal work
from Phase 4 are implemented. The bounded mesh-flow hydraulic model from Phase
5 failed visual validation: it retained rivers but lost the characteristic
hydraulic drainage and ridge relief. The proven path-based model is therefore
the default again. Mesh flow remains opt-in through
`MOTU_EXPERIMENTAL_MESH_FLOW=1` only for further investigation.

The rejected mesh-flow checkpoint for seed 666 with 1,024 input points was a
6.684 s median across three warmed release runs using 14 Rayon workers. That
number must not be presented as the performance of the accepted-quality
generator. An initial warmed release run of the restored path model completed
in 17.07 s with 2,069,165 vertices, 4,138,183 triangles, 96 rivers, and the
expected detailed morphology. This meets the two-times whole-generation target
against the 34.32 s baseline, but it is not yet a three-run median.

The rebuilt release plugin has been copied into Unity and the batch native
interop validation passes. High-detail surface maps keep their existing direct
bakes: lower LOD maps correct against different target meshes, so blindly
downsampling the LOD0 result would change their meaning. Their existing
row-parallel implementation remains in place.

The primary remaining optimization problem is the accepted path-based hydraulic
stage. Directly parallelizing source paths remains invalid because paths mutate
overlapping vertices and later routes intentionally observe earlier changes.
A persistent all-purpose `MeshConnectivity` and parallel coastal attack were
not retained because the
coastal stage changes topology internally and profiling shows it at about 0.85
s total versus about 4.35 s for hydraulic erosion. Tessellation buffers are
right-sized and use packed-edge hashing, but are not retained between topology
generations because their output mesh becomes the next generation's input.

This document is the implementation contract for reducing Rust island-generation
time and peak memory while retaining the free-form mesh, adaptive detail,
hardness-aware material model, hydraulic and coastal erosion, river shaping,
waterfalls, LOD correction, texture baking, Unity slicing, and collider streaming.

Render-only cliff sharpening is explicitly abandoned. It has remained disabled
because it can invert steep terrain and has not produced useful results. The
implementation should delete that feature and its duplicate render mesh rather
than preserve it as a dormant optimisation target.

## Objective

Make the default Rust and Unity generation path substantially faster by:

- removing work that Unity does not consume;
- removing duplicate multi-million-vertex mesh storage;
- reducing repeated topology construction and oversized temporary buffers;
- parallelising independent, deterministic vertex-local work;
- replacing the path-per-vertex hydraulic algorithm with a bounded mesh-flow
  simulation; and
- preserving geometric, material, river, LOD, and seam invariants.

The primary performance target is at least a two-times reduction in native
island-generation time at the default 1,024 seed points. The initial stretch
target is a default `Island::generate` time of 10 seconds or less on the current
development Mac. Targets must be evaluated after every phase and revised from
measured evidence rather than met by lowering terrain detail silently.

## Measured Starting Point

The review used an optimized build and a 1x1 output raster so image rendering
did not dominate generation.

| Seed points | Final terrain vertices | Release time |
| ---: | ---: | ---: |
| 128 | 291,729 | 0.83 s |
| 512 | 980,690 | 7.17 s |
| 1,024 | 2,069,165 | 34.32 s |

The default 1,024-point run used approximately 33.75 seconds of user CPU in
34.32 seconds elapsed. Generation is therefore almost entirely single-threaded.
The machine reports 14 available logical processors.

Sampling identified two dominant execution windows:

1. hydraulic erosion, including repeated downhill-neighbour searches, live
   normal reconstruction, and projected-face safety checks; and
2. eager decoration placement, including two surface lookups per candidate and
   an all-river-node proximity scan.

The final mesh size grows much faster than the input seed count. Runtime grows
faster again because several stages walk paths or rebuild topology over that
mesh. The dominant problem is algorithmic scaling and memory bandwidth, not a
small allocator call inside the hydraulic path loop.

Before implementation begins, rerun the baseline three times after one warm-up
run and record the median, minimum, final vertex and triangle counts, river
count, and peak resident memory. The numbers above are orientation data, not a
substitute for that controlled baseline.

## Scope

### In scope

- Rust generation, simulation, derived-data, and FFI preparation performance.
- Removal of cliff-sharpening code, controls, tests, and compatibility storage.
- Lazy construction of optional decorations.
- Surface-query and river-proximity indexing.
- Tessellation and topology scratch-buffer reuse.
- Deterministic CPU parallelism.
- A deterministic batched hydraulic-flow redesign.
- Release-profile tuning after algorithmic work.
- Native plugin rebuild and Unity integration validation.

### Out of scope

- Reducing final detail merely to satisfy a timing target.
- Replacing the free-form triangle mesh with a height grid.
- GPU compute in the first implementation.
- Concurrent generation of multiple islands inside one Unity viewer.
- Reintroducing render-only cliff sharpening under a different name.
- Parallel source-path mutation using locks, atomics, or unchecked raw pointers.

## Non-negotiable Behaviour

The optimized generator must retain:

- deterministic results for a fixed seed, options, build, and worker count;
- coherent hardness derived from the initial terrain-noise field;
- separate soft deposited material and resistant bedrock;
- loose material being removed before bedrock;
- conservative material accounting through tessellation and river transport;
- normal/Z hybrid hydraulic retreat and slope-dependent deposition;
- the projected-face inversion protection;
- adaptive refinement in high-displacement regions;
- final LOD prefix correction and cross-LOD seam alignment;
- river beds carved into terrain and a river surface duplicated from terrain;
- waterfall bed shelves, frequent gradient-dependent steps, and rounded top lips;
- final river topology without uphill bank loops;
- coastal geology, exposed soft-cover removal, beaches, and headlands;
- existing normal and occlusion map resolutions; and
- Unity's background-generation and tile-streaming behaviour.

## Architectural Principles

### Separate topology from geometry

Adjacency, incident faces, perimeter flags, and face adjacency depend on
triangle connectivity, not vertex positions. Moving vertices must not rebuild
them. Tessellation is the point at which topology caches become invalid.

Introduce an internal connectivity object, for example:

```rust
struct MeshConnectivity {
    adjacency: Adjacency,
    vertex_faces: VertexFaceAdjacency,
    perimeter: Vec<bool>,
}
```

Construct it once after a topology-changing stage and pass it by shared borrow
to erosion, smoothing, rivers, normals, and coast operations until the next
tessellation. Add face adjacency only where coastal work demonstrates that it
belongs in the shared cache.

### Reuse scratch storage without sharing mutable mesh state

Introduce stage-specific scratch types owned by the generation orchestrator:

```rust
struct GenerationScratch {
    hydraulic: HydraulicScratch,
    tessellation: TessellationScratch,
    surface_query: SurfaceQueryScratch,
}
```

Scratch vectors should use `clear`, `fill`, `resize`, and retained capacity.
They must not be stored in public `Mesh` or exported through the C ABI. A stage
borrows the mesh and its scratch for the duration of the operation; the mesh
remains the sole owner of terrain geometry.

### Use snapshot, calculate, reduce, apply phases

Parallel stages must not mutate shared mesh vertices while other workers read
them. Use this shape:

1. borrow immutable input geometry and material;
2. calculate one ordered result per vertex/face/source in parallel;
3. reduce cross-vertex contributions deterministically; and
4. apply results through one mutable phase.

Do not wrap the mesh in `Arc<Mutex<_>>`. Locking individual vertices would add
contention and would not reproduce the existing source ordering.

### Restrict unsafe code to proven disjoint writes

Unsafe code is permitted but is not the concurrency design. Use it only if a
profile proves bounds checks or compiler alias analysis remain significant
after the algorithmic work. Every unsafe parallel write must have a documented
proof that no two workers can reach the same element, plus a safe reference
implementation used by tests.

## Phase 0: Reproducible Measurement Harness

### Work

1. Add a generation benchmark that invokes `Island::generate` without raster,
   PNG, texture, slicing, or Unity upload work.
2. Add optional stage timing behind a non-default `profiling` feature or a
   dedicated benchmark binary. Production generation must not format log
   messages or call the clock inside hot loops.
3. Record at least these stages:
   - base Delaunay and elevation assignment;
   - each tessellation;
   - each connectivity construction;
   - each hydraulic stage;
   - each thermal erosion stage;
   - each river generation and shaping stage;
   - coarse and detail coastal stages;
   - final river mesh refinement;
   - LOD correction;
   - terrain query-index construction; and
   - decorations.
4. Record allocation/peak-memory information separately where the platform can
   provide it without affecting the timing run.
5. Add a Unity preparation timer breakdown for:
   - `CreateMotu`;
   - LOD0, LOD1, and LOD2 surface maps;
   - overview mesh slicing;
   - river mesh slicing; and
   - managed copying and Unity upload.
6. Use fixed seeds `666`, `2018`, and at least three additional seeds selected
   before optimization. Retain their options and benchmark commands in this
   file or the benchmark README.

### Acceptance criteria

- Three warmed repetitions produce a median for every recorded configuration.
- Timing output distinguishes native island generation from maps and slicing.
- Instrumentation changes default release runtime by less than 1% when disabled.
- Final vertex/triangle count, river count, and a stable geometry hash accompany
  each timing result.

## Phase 1: Delete Cliff Sharpening and Duplicate Render Geometry

### Work

1. Delete `src/render_mesh.rs` and the `RenderMesh` field from `Island` once its
   still-required clipping behaviour has been moved to `Mesh`.
2. Route LOD0 display and slicing through the same support-mesh paths used by
   the other LODs. Because LOD0 display geometry equals support geometry, its UV
   and clamp source are already known.
3. Preserve exact clipping at tile boundaries and outer-edge projection onto
   LOD1. Do not change the rule that only edges facing a coarser neighbour are
   clamped.
4. Remove:
   - `cliff_render_strength` from Rust `IslandOptions`;
   - the matching native `MotuOptions` field and C header declaration;
   - the Unity `Options` field and assignments;
   - save-file writes for the value;
   - true-3D cliff tests, constants, and compatibility comments.
5. Continue reading old save versions that contain the obsolete float if saved
   islands remain useful, but discard it. New saves should use a bumped format
   without the field.
6. Stop returning `GeologyField` from base generation solely for the removed
   render path. `SurfaceMaterial` keeps the sampled hardness values needed by
   simulation.

### Acceptance criteria

- LOD0 render vertices and triangles are identical to the corrected support
  LOD0 before and after removal.
- Existing seam and grid-slicing tests pass for all LOD transitions.
- Native and Unity option layouts match after rebuilding both sides.
- The island no longer owns a second full LOD0 mesh or a second support-position
  vector.
- Peak memory and Phase 1 runtime are measured and recorded.
- No source symbol, UI control, save field, or test still suggests that cliff
  sharpening is supported.

## Phase 2: Make Decorations Lazy and Fix Surface Queries

### Work

1. Replace eager `Decorations` construction in `Island::generate` with
   `OnceLock<Decorations>` or an equivalent single-assignment cache.
2. Initialize it only when decorations, foliage data, or tree billboard APIs
   are called. The FFI must continue returning pointers whose storage remains
   stable until the island is released.
3. Change decoration placement to call `Terrain::sample_surface` once per
   candidate rather than performing separate height and normal lookups.
4. Add a compact uniform spatial index over river-node XY positions. A
   decoration candidate should inspect only its own cell and neighbouring cells
   whose distance can intersect the exclusion radius.
5. Replace `nearest_vertex_index`, which scans every terrain vertex on a query
   miss, with a bounded spatial-index fallback:
   - first inspect triangle candidates in the requested bin;
   - expand through neighbouring bins until a non-empty ring is found;
   - test the unique vertices referenced by those faces; and
   - use the full-mesh scan only as a debug assertion fallback, never as the
     normal release path.
6. Reassess the hard maximum dimension of 512 in `TriangleIndex`. Select the
   dimension from face count and measured candidate occupancy, with a memory cap
   based on bytes rather than an arbitrary bin count.
7. Reserve decoration output capacity based on the target and observed category
   ratios, without allocating the target capacity independently for every type.

### Acceptance criteria

- `Island::generate` performs no decoration candidate sampling when Unity does
  not request decorations.
- The first decoration/foliage request initializes exactly once and subsequent
  calls reuse the same storage.
- Fixed-seed decoration output is deterministic.
- Surface queries over a dense test grid do not enter an O(vertex-count)
  fallback.
- Surface height/normal results remain within the existing floating-point
  tolerance.
- Unity `CreateMotu` succeeds without paying decoration cost.
- Stage benchmarks record lazy and requested-decoration timings separately.

## Phase 3: Reduce Tessellation and Connectivity Overhead

### Tessellation work

1. Evaluate the split predicate once per source face and store a compact split
   mask.
2. Count selected faces and selected edges before reserving output buffers.
   Replace unconditional `source_triangle_indices * 4` capacity with a bound
   derived from selected and conforming faces.
3. Replace the midpoint `BTreeMap<(u32, u32), ...>` with a pre-sized hash table
   keyed by the existing packed `u64` ordered-edge representation, unless a
   benchmark proves a sorted edge-vector approach faster and smaller.
4. Reuse midpoint maps, split masks, new-vertex stencils, and triangle output
   capacity through `TessellationScratch`.
5. Preserve the invariant that original vertices remain an unchanged prefix and
   new midpoint attributes are derived only from the preceding generation.
6. Avoid cloning data that is not required by the new LOD. Retain the vertex and
   UV prefix copies that are genuinely required because coarser LODs remain
   alive.

### Connectivity work

1. Build `MeshConnectivity` immediately after each topology change.
2. Pass the same adjacency to smoothing, hydraulic, thermal, river, and coastal
   work until another tessellation occurs.
3. Reuse incident-face offsets in normal reconstruction and the hydraulic
   projected-area guard.
4. Reuse perimeter flags instead of sorting all triangle edges repeatedly.
5. Keep geometry-dependent arrays such as current projected face areas outside
   the persistent connectivity object and refresh only affected values.

### Acceptance criteria

- Tessellation produces identical topology, vertex order, midpoint attributes,
  and corrected LOD prefixes for the fixed regression seeds.
- Material volume before and after tessellation remains within its current
  tolerance.
- Connectivity is rebuilt only after an operation that changes triangle
  topology.
- No per-face ordered-tree insertion remains in adaptive tessellation unless it
  wins a recorded benchmark.
- Peak temporary capacity and time per tessellation are recorded before and
  after the phase.

## Phase 4: Parallelise Exact or Naturally Independent Work

Use one reusable Rayon pool, or one equivalent persistent worker pool, rather
than spawning new operating-system threads for every small loop. The Unity
viewer prevents concurrent island generation, so one process-wide pool is an
appropriate first design. Retain a single-thread execution mode for reference
testing.

### Candidate stages

#### Surface noise

Apply noise independently with indexed parallel iteration over paired vertex
and normal slices. Noise is a pure function of seed and position, so output can
remain bit-identical.

#### Smoothing

Calculate each destination vertex from immutable source vertices and shared
adjacency in parallel. Preserve ordering by collecting into an indexed output
slice, then swap the completed vertex buffer into the mesh.

#### Thermal erosion

Split the current pass into:

1. a parallel calculation of one `ThermalTransfer` per source vertex; and
2. a deterministic source-index-order reduction into geometry and loose-cover
   delta arrays.

The second phase can reproduce existing floating-point addition order exactly.

#### River analysis

Parallelise downstream-neighbour selection, local slope calculations, and
other read-only per-vertex classification. Keep flow accumulation and source
ordering deterministic.

#### Coastal erosion

Calculate per-vertex attack, eligibility, and requested delta in parallel.
Reduce owner-indexed sediment volumes deterministically before longshore
transport and beach allocation. Do not use floating-point atomics.

#### Normal calculation

Prefer vertex-parallel gathering through incident-face adjacency over
face-parallel scattering into shared vertex normals. This uses topology already
held by `MeshConnectivity` and needs no per-worker full-size normal buffer.

#### Surface maps and raster

They are already threaded. Move them to the common pool only if it reduces
thread-management overhead or simplifies cancellation without reducing
throughput.

### Acceptance criteria

- Single-thread and multi-thread modes produce identical hashes for stages that
  claim exact preservation.
- Repeated multi-thread runs produce identical output.
- Thread sanitizer or an equivalent focused stress test finds no shared mutable
  races in FFI generation.
- Each parallelised stage improves its own median time by at least 20%, or the
  parallel code is reverted as unjustified complexity.
- Whole-generation performance is measured after every stage to avoid optimizing
  work that has ceased to matter.

## Phase 5: Replace Path-per-Vertex Hydraulic Erosion

### Why direct source parallelism is rejected

The existing hydraulic loop mutates terrain height, XY position, loose cover,
carried sediment, and projected triangle areas immediately. Two source paths
can visit the same vertex or neighbouring vertices of the same face. Running
those paths concurrently would make downhill routing, material availability,
face safety, and output depend on scheduling.

Locks would serialize the common drainage routes and could deadlock if several
vertices/faces were acquired in different orders. Floating-point atomics would
be nondeterministic. Domain tiles would stop or duplicate water at boundaries
and create erosion seams. Unsafe pointers would merely hide the data race.

### Proposed hydraulic model

Replace each current stage with a small number of deterministic mesh-flow
iterations. One iteration uses a stable geometry snapshot and contains these
phases.

#### 1. Rainfall/source weighting

Compute projected vertex control areas and assign water proportional to physical
surface area, optionally modulated by the existing deterministic rainfall/noise
field. This removes the current resolution dependence in which every new
tessellated vertex creates another full downhill path.

#### 2. Downstream map

In parallel, choose the steepest valid downhill neighbour for each vertex from
snapshot geometry. Preserve deterministic vertex-index tie-breaking. Treat sea
and sinks according to the current stage setting.

#### 3. Flow accumulation

Process vertices in descending snapshot height and add each vertex's water to
its downstream neighbour. Begin with a sequential ordered accumulation because
it is linear and deterministic. Only parallelise it later if profiling shows it
is significant.

Reuse the river system's downstream-map and flow-accumulation concepts where
their contracts match, but keep hydraulic water as floating-point physical
volume rather than reusing the river source count blindly.

#### 4. Sediment exchange

For each vertex in downstream order:

- receive upstream water and sediment;
- calculate velocity/capacity from flow, gradient, and local distance;
- compute deposition weight from slope;
- remove loose material before bedrock;
- apply the cached hardness erosion rate only to the bedrock remainder;
- retain the normal-to-Z hybrid erosion direction;
- pass remaining water and sediment downstream; and
- record requested geometry and loose-cover deltas without mutating the mesh.

Each vertex is exchanged once per iteration instead of once for every upstream
source path.

#### 5. Geometric safety and application

The existing per-move projected-face cap must remain. Applying all vertex
motions simultaneously cannot assume that individually safe motions are safe
when all three vertices of a face move.

Implement in this order:

1. deterministic sequential application through the existing incident-face
   guard;
2. after correctness and performance are established, optionally greedily
   colour vertices so vertices in one colour share no face;
3. process one colour at a time, with disjoint vertices calculated in parallel;
   and
4. update current projected areas between colours.

Do not add graph colouring unless the sequential apply phase remains measurable.
If unsafe disjoint writes are eventually used within one colour, document and
test the no-shared-face proof.

#### 6. Sink and fan deposition

Replace path-local fan calls with a final sink/outlet deposition phase. Sum the
sediment arriving at each sink, calculate the fan weights from snapshot
geometry, reduce overlaps deterministically, and update both geometry and
`deposited_depth` by the exact applied amount.

#### 7. Iteration and calibration

Rebuild geometry-dependent normals, downstream choices, and current projected
areas between iterations, while retaining connectivity. Start with a small
fixed iteration count per LOD stage and calibrate stage strength against the
current output.

Do not simulate quality by restoring one iteration per vertex. Increase the
number of global iterations only while fixed-seed morphology measurably
improves.

### Hydraulic scratch layout

Use contiguous buffers retained across stages:

```rust
struct HydraulicScratch {
    order: Vec<u32>,
    downstream: Vec<u32>,
    water: Vec<f32>,
    sediment: Vec<f32>,
    requested_shift: Vec<Vec3>,
    loose_delta: Vec<f32>,
}
```

Avoid one dense buffer per worker: at roughly two million vertices, multiplying
several arrays by 14 workers would consume hundreds of megabytes. Avoid sparse
per-droplet maps because they recreate allocator pressure and require an
expensive merge.

### Hydraulic acceptance criteria

- Runtime for all hydraulic stages improves by at least four times at the
  default configuration, or the phase requires explicit review before merge.
- Whole `Island::generate` time improves by at least two times from the Phase 0
  baseline unless earlier phases have already made hydraulic erosion minor.
- Results are deterministic across repeated runs and supported worker counts.
- No projected face reverses beyond the established `1e-10` numerical tolerance.
- No NaN or infinity appears in vertices, normals, water, sediment, hardness, or
  loose cover.
- Loose cover is removed before bedrock and hard bedrock remains slower to erode.
- Erosion peaks on intermediate slopes and fades at horizontal and vertical
  orientations as currently designed.
- Deposited material remains soft and total material error remains within the
  existing tolerance after each iteration and tessellation.
- Rivers still form downhill broad valleys, reach the sea, and do not become
  blocked by hydraulic deposits.
- Fixed-seed visual comparisons retain cliffs, overhang-related normal noise,
  beaches, headlands, river valleys, waterfalls, and deltas.

## Phase 6: Derived Assets and Unity Preparation

### Work

1. Benchmark the three surface-map resolutions after query-index fixes.
2. Build the high-detail surface samples once at the highest useful resolution
   and assess whether lower-resolution LOD maps can be deterministically
   downsampled without changing their intended target-normal correction. Reuse
   only data that has the same semantic meaning.
3. Reuse triangle indices for LOD0, LOD1, and LOD2 across maps, height sampling,
   and decoration generation.
4. Keep the 64x64 river grid as one source-mesh pass. Pre-size only non-empty
   tile storage where possible rather than allocating a large hash map for every
   empty tile.
5. Apply cancellation checks between native preparation stages. Fine-grained
   cancellation inside Rust hot loops is optional and must be benchmarked.
6. Preserve the rule that Unity API and resource creation remain on Unity's
   main thread while native CPU work remains in the background task.

### Acceptance criteria

- Surface maps remain 2048, 1024, and 512 pixels and preserve expected byte
  lengths and texture formats.
- LOD0 occlusion remains applied.
- Normal and occlusion maps remain deterministic for fixed inputs.
- River tile geometry and UV arrays pass native ownership validation.
- Unity remains responsive during generation and displays stage-specific timing.
- End-to-end background preparation improves without moving Unity resource
  calls off the main thread.

## Phase 7: Release Build Tuning

Only after algorithmic phases are complete, benchmark:

```toml
[profile.release]
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

Also benchmark the locally built macOS Unity plugin with
`-C target-cpu=native`. Because that artifact is built for the current Mac, CPU
specialization is acceptable if the documented build command remains clear.

Inspect generated assembly or use a focused microbenchmark before introducing
unchecked indexing. Prefer algorithmic and cache-layout improvements whenever
they dominate bounds checks.

### Acceptance criteria

- Every retained compiler setting has an independently measured improvement.
- The plugin loads on the supported Unity/macOS architecture.
- Release tuning does not alter deterministic geometry or texture hashes.
- Panic/FFI behaviour remains defined; no Rust panic unwinds through C#.

## Validation Matrix

### Rust correctness

Run after each implementation phase:

```text
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
git diff --check
```

Focused regressions must cover:

- adaptive tessellation vertex-prefix and stencil lineage;
- loose-material volume conservation;
- hardness ordering and loose-before-bedrock removal;
- projected face orientation and minimum area;
- hydraulic determinism at one and multiple workers;
- surface-query fallback complexity;
- lazy decoration initialization and FFI pointer lifetime;
- LOD correction and tile seams;
- river downhill routing and loop rejection;
- waterfall lip rounding only at the top;
- coast sediment conservation; and
- normal/occlusion byte lengths and deterministic hashes.

### Performance

Measure release builds, after warm-up, with no other benchmark running. Record:

- median and minimum of at least three runs;
- wall, user, and system time;
- peak resident memory where available;
- stage timings;
- final vertex/triangle/river counts;
- worker count; and
- compiler/profile settings.

Performance tests are manual gates, not timing assertions in ordinary CI.

### Visual quality

For each selected fixed seed, capture the same Unity views:

- full-island overview with mesh lines off and on;
- exposed rocky headland;
- sheltered beach;
- steep hydraulic cliff/ridge;
- broad river valley;
- river mouth/delta; and
- stepped waterfall from above and first-person ground level.

Compare before/after images and mesh statistics. Any major hydraulic redesign
requires user visual approval before deleting its reference implementation.

### Unity/native integration

After Rust validation:

1. rebuild the release `libmotu.dylib`;
2. copy it to `island-unity/Assets/Plugins/macOS/libmotu.dylib`;
3. restart Unity so the native library is reloaded;
4. run the batch native interop validation;
5. generate in the editor with default and strong hydraulic settings;
6. enter first-person mode and cross LOD tile boundaries;
7. verify collider streaming, river visibility, mesh-line display, and texture
   application; and
8. record end-to-end timings separately from Unity upload time.

## Sequencing and Checkpoints

Implement and commit in this order:

1. measurement harness;
2. cliff-sharpening deletion and render-mesh deduplication;
3. lazy decorations and surface-query fixes;
4. tessellation/connectivity reuse;
5. exact parallel stages;
6. hydraulic-flow redesign behind an internal comparison switch;
7. remove the reference hydraulic path only after quality approval;
8. derived-asset/Unity preparation improvements; and
9. compiler tuning.

After each phase:

- run correctness validation;
- capture benchmark results;
- inspect the diff for accidental detail reductions or ABI drift;
- rebuild Unity only when the native boundary or behaviour changed; and
- stop and reassess if runtime, memory, determinism, or visual quality regresses.

## Expected Outcome

The earliest phases should remove unnecessary Unity work and large duplicate
storage without altering terrain. Connectivity and tessellation work should
reduce memory bandwidth and repeated allocation. Parallel vertex-local stages
should then use the available cores without changing simulation ordering.

The major speedup is expected from the hydraulic redesign: physical rainfall
and flow should be proportional to terrain area rather than the number of
tessellated vertices, and every vertex should be processed a bounded number of
times per iteration rather than once for every upstream source. This should
make higher detail much less expensive while retaining the geological and river
features that motivated the current mesh density.

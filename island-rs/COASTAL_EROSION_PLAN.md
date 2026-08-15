# Coastal Erosion and Beach Formation Plan

## Status

This document is the implementation contract for adding mesh-native coastal
evolution to `island-rs` and exposing its primary controls in the Unity viewer.
It records the intended design so the work can continue consistently across
separate sessions and context compaction.

The first complete implementation was added in August 2026. It includes the
shared geology field, sea-level contour topology, mesh-adjacency wave fetch,
hardness-weighted erosion, conservative longshore sediment transport,
exposure-dependent beaches, selective coastal refinement, coarse/detail
pipeline stages, Rust/native/Unity controls, and focused invariants. The
remaining empirical work is multi-seed Unity visual tuning after restarting the
editor with the rebuilt native plugin.

The first visual tuning follow-up moved the lasting pass after broad land
smoothing, increased the hardness contrast, and separated broad soft-rock
retreat from narrow hard-rock toe undercutting. Hard coastal vertices are also
excluded from broad beach deposition. This prevents later smoothing and beach
profiles from turning resistant headlands into uniformly gentle slopes.

The second visual follow-up added a separate vertex-aligned unconsolidated
cover thickness. Gentle existing near-shore sediment initializes that layer,
new coastal deposition increases it, and erosion consumes it before consulting
bedrock hardness. An exposure-only cleanup after the final deposition step uses
a larger edge-relative removal cap for loose material, preventing the last
beach pass from wrapping exposed cliffs while retaining sheltered beaches.

## Objective

Add a deterministic coastal process that naturally produces:

- resistant rocky headlands;
- eroded bays and coves;
- broad beaches in sheltered, low-energy locations;
- narrow or absent beaches on exposed rocky coastlines;
- shallow wave-cut platforms offshore of eroding cliffs;
- conservative longshore transport of eroded sediment; and
- coastlines that remain compatible with the free-form triangle mesh, adaptive
  tessellation, rivers, all three LODs, Unity terrain slicing, and collider
  streaming.

The result must evolve the coastline rather than merely smooth coastal
vertices, flatten an arbitrary strip beside the sea, or generate a grid-based
terrain replacement.

## Existing System

### Current Rust terrain generation

`src/terrain.rs` currently:

1. Creates random XY seed points and a Delaunay mesh.
2. Computes a terrain score from broad continental noise, detail noise, and a
   radial falloff that guarantees an island surrounded by sea.
3. Chooses sea coverage by ranking the terrain scores.
4. Converts the land/sea classification into elevation using graph distance.
5. Applies staged smoothing, hydraulic and thermal erosion, river shaping, and
   adaptive tessellation. The former normal-directed surface-noise passes were
   removed after the shared geology-hardness field made their visual effect
   redundant.
6. Generates the definitive river network and water mesh on LOD0.
7. Copies shared final LOD0 vertex positions back into LOD1 and LOD2.

There is currently no dedicated wave erosion, shoreline sediment transport, or
equilibrium beach-profile stage.

### Relevant original C++ behavior

The original project contains experimental operations named
`applySeaErosian`, `eatCoastlines`, `smoothCoastlines`, and `formBeaches`.
Those operations use local land/sea neighbour counts and direct elevation
changes. They are not used by the original island generation pipeline. The
pipeline only repeatedly calls `improveCliffs`, which shifts and sharpens local
coastal geometry.

The new implementation should preserve the useful intent, not port those
operations literally. In particular, the new process must avoid the original
coast-following assumptions, which can produce ambiguous paths on an
irregular triangulation.

## Core Design Decisions

### 1. The free-form mesh remains authoritative

All terrain mutation occurs on the irregular triangle mesh. A heightmap or
raster may be produced later as a derived Unity collider input, but it must not
define coastal connectivity or replace the mesh during erosion.

Wave fetch will be traced through triangle adjacency rather than through a
terrain grid. This keeps the simulation aligned with the actual topology and
avoids resolution-dependent raster artifacts.

### 2. Geology and initial terrain share the same noise field

Rock hardness must be visibly related to the noise that created the initial
terrain. The initial elevation score and the coastal resistance field will use
the same sampled components:

```text
continental = broad fractal noise at the existing continental frequency
detail      = finer fractal noise at the existing detail frequency

height score = 0.78 * continental
             + 0.22 * detail
             + radial island falloff

raw hardness = 0.80 * continental
             + 0.20 * detail
```

The exact weights may be tuned from generated-island comparisons, but the
continental component must dominate both calculations.

The radial falloff is deliberately excluded from hardness. It exists to make
the generated land an island, not to describe rock. Including it would make
every outer coast artificially soft and radially uniform.

Hardness is sampled in world XY space, so newly tessellated vertices naturally
receive values consistent with their neighbours. It must not be generated as
independent per-vertex randomness.

The raw hardness field will be normalized with calibration values taken from
the initial base-mesh samples, then passed through a smooth nonlinear remap:

```text
normalized = clamp((raw - low_percentile) / (high_percentile - low_percentile), 0, 1)
hardness   = normalized * normalized * (3 - 2 * normalized)
```

Using broad percentiles rather than raw theoretical noise bounds makes the
available resistance range stable across seeds. The smoothstep curve produces
large moderate-resistance regions with coherent hard and soft patches.

The extraction of the shared noise helper must preserve current terrain-score
results. With coastal erosion disabled, an island generated before and after
the refactor must have the same initial land/sea classification and elevations.

### 3. Shorelines are triangle/sea-level intersections

A shoreline is not defined as a chain of vertices whose elevations happen to
be near zero. For every triangle that crosses sea level, intersect its edges
with the `z = 0` plane. Each normal coastal triangle contributes one contour
segment.

Intersection points are keyed by their source mesh edge:

```rust
struct ShorePoint {
    edge: [u32; 2],
    interpolation: f32,
    position: Vec3,
}

struct ShoreSegment {
    points: [u32; 2],
    triangle: u32,
    sea_side_triangle: u32,
}
```

Shared edge keys join the segments into ordered closed shoreline loops. This
representation:

- follows the rendered sea-level contour precisely;
- remains smooth when the shoreline crosses the interiors of large faces;
- has no arbitrary choice between several neighbouring coastal vertices;
- supplies an ordered tangent for curvature and longshore transport; and
- retains a direct mapping back to the two mesh vertices that receive forces.

Use an elevation epsilon and deterministic vertex-index tie-breaking when a
vertex lies exactly at sea level. The island perimeter is expected to be
underwater, so primary coastlines should form closed loops. Open or malformed
contours must be detected and skipped safely rather than followed indefinitely.

### 4. Headlands require both wave exposure and coherent resistance

Uniform wave erosion alone eventually rounds the island. Natural persistent
headlands emerge from the interaction of:

- coherent hard rock inherited from the initial continental noise;
- softer neighbouring material retreating into bays;
- increased open-water exposure around convex points; and
- reduced fetch and wave incidence inside sheltered bays.

Hardness controls the erosion rate but does not make any vertex completely
immune. A useful starting relationship is:

```text
erodibility = minimum_erodibility + (1 - hardness)^hardness_exponent
```

The minimum term permits slow erosion of hard headlands, while the exponent
creates a strong difference between soft and hard regions.

### 5. Sediment is transported, not invented locally

Every lowering operation estimates removed volume using a vertex's projected
dual area. Removed volume enters a shoreline sediment budget. That budget is
advected along the ordered shoreline and deposited only where an equilibrium
profile has space for it.

The accounting relationship is:

```text
eroded volume = deposited volume + exported offshore volume + retained load
```

Minor floating-point tolerance is acceptable. Material must not disappear
silently during transport, and beach formation must not create unlimited
terrain volume.

## Proposed Data Model

Add a private coastal module, initially `src/coast.rs`, instead of expanding
`terrain.rs` with all implementation details.

Suggested internal types:

```rust
struct TerrainNoiseSample {
    continental: f32,
    detail: f32,
}

struct GeologyField {
    seed: u64,
    low_raw_hardness: f32,
    high_raw_hardness: f32,
}

struct CoastTopology {
    points: Vec<ShorePoint>,
    segments: Vec<ShoreSegment>,
    loops: Vec<ShoreLoop>,
}

struct ShoreLoop {
    points: Vec<u32>,
    cumulative_length: Vec<f32>,
    signed_area: f32,
}

struct CoastalBand {
    distance_to_shore: Vec<f32>,
    nearest_shore_point: Vec<u32>,
    signed_side: Vec<i8>,
    dual_area: Vec<f32>,
}

struct WaveClimate {
    directions: Vec<Vec2>,
    weights: Vec<f32>,
}

struct CoastIterationStats {
    eroded_volume: f64,
    deposited_volume: f64,
    exported_volume: f64,
    retained_volume: f64,
}
```

These are generation-time scratch structures. They are not part of the public
mesh, serialized island, or C ABI.

`WaveClimate` should use a small fixed-size representation in the final code
if profiling shows a benefit. The design should not allocate a new collection
per shoreline point, ray, vertex, or iteration.

## Algorithm

### Phase A: Shared terrain and geology sampling

1. Extract the continental and detail noise calculations from
   `assign_elevations` into a shared deterministic sampling function.
2. Keep the current seed constants, frequencies, octave counts, and height
   weights unchanged.
3. Compute initial raw-hardness samples over the base mesh.
4. Select robust low/high calibration percentiles without cloning or sorting
   large refined meshes. Sorting the approximately 1,024 initial samples is
   acceptable.
5. Construct `GeologyField` from the seed and calibration values.
6. Use the shared noise sample for current terrain scoring.
7. Expose only a private `hardness(Vec2) -> f32` method to the coastal pass.

Acceptance requirements for Phase A:

- Coastal erosion disabled produces the existing terrain classification and
  elevation results for fixed seeds.
- Repeated hardness sampling at the same seed and XY is deterministic.
- Newly inserted edge midpoints have hardness between or spatially consistent
  with nearby samples; there is no salt-and-pepper variation.
- The measured correlation between broad terrain noise and hardness is strong
  and positive in a deterministic unit test.

### Phase B: Coastal topology and active band

1. Build a mesh edge-to-face table once for the stage.
2. Extract all sea-level contour segments from crossing triangles.
3. Chain shared contour points into oriented loops.
4. Determine the seaward side of each segment from triangle classification.
5. Compute tangent, seaward normal, local segment length, and signed turning
   angle for each ordered contour point.
6. Smooth tangents and turning angles along each loop using a short weighted
   window to prevent individual triangle shapes from dominating exposure.
7. Seed a multi-source Dijkstra traversal from shoreline-crossing edge
   endpoints to build a narrow geodesic coastal band.
8. Store the nearest shoreline point and signed land/sea side for each band
   vertex.
9. Compute projected dual area as one third of the XY area of every incident
   triangle.

The active band should be wide enough to contain the largest possible beach
profile plus one iteration's maximum shoreline retreat. Distances are in the
normalized island coordinate system and should be converted from named
physical intentions, such as an approximately 20–100 metre coastal zone on a
2 km island, rather than being tied to vertex-ring counts.

Acceptance requirements for Phase B:

- A synthetic convex island produces one closed loop with degree two at every
  contour point.
- Multiple islands/holes, if supplied to the utility test, produce independent
  deterministic loops.
- Exact-sea-level vertices do not create duplicate points or infinite loops.
- Signed side and seaward normal are consistent around clockwise and
  counter-clockwise input triangle windings.
- Dual areas are finite, positive for interior vertices, and sum to the
  projected mesh area within tolerance.

### Phase C: Mesh-native wave exposure

Use a small deterministic directional wave climate. The first implementation
should use 12 or 16 directions. A seed-derived prevailing direction and broad
directional spread should coexist with a weaker omnidirectional background so
all open coast receives some energy.

For every shoreline point and wave direction:

1. Reject directions that approach from the landward side.
2. Start in the known seaward triangle.
3. Walk the ray from triangle to adjacent triangle by crossing the next face
   edge in XY.
4. Stop when the ray reaches another land crossing, the mesh perimeter, or a
   configured maximum fetch.
5. Record open-water fetch and incidence against the local seaward normal.

A starting exposure model is:

```text
directional energy = climate weight
                   * sqrt(fetch / maximum fetch)
                   * incidence^2

exposure = sum(directional energy)
```

Normalize exposure per generation into a stable working range. Apply one or
two short, simultaneous smoothing passes along each shoreline loop.

Fetch naturally shelters bays and the leeward side of headlands. A restrained
curvature factor may enhance wave focusing at convex capes, but it must remain
secondary to fetch and incidence:

```text
focused exposure = exposure * clamp(1 + focus * signed_curvature, min, max)
```

Acceptance requirements for Phase C:

- An open straight synthetic coast receives greater exposure than the same
  coast behind a headland.
- A convex cape receives more exposure than the back of an otherwise
  equivalent concave bay.
- Rotating the prevailing direction rotates the high-exposure side.
- Wave rays terminate in bounded time and never allocate during triangle
  walking.

### Phase D: Wave erosion and wave-cut platforms

For each coastal-band vertex, derive local exposure from its nearest shoreline
point and sample geology at the vertex's XY position.

The erosion envelope depends on:

- wave exposure;
- erodibility from shared hardness;
- geodesic distance to the shoreline;
- elevation relative to sea level; and
- the configured coastal erosion strength.

Apply the strongest attack close to sea level, tapering above storm run-up and
below wave-base depth. Use smooth falloffs rather than hard band boundaries.

Compute all vertex elevation deltas into reusable scratch arrays and apply them
simultaneously. Do not mutate a vertex while neighbouring erosion decisions
are still being calculated.

Landward vertices may be lowered through sea level, which makes the shoreline
retreat. Shallow seaward vertices may be cut down toward a gently sloping
wave-cut platform. Erosion must never raise terrain.

Removed volume is approximately:

```text
removed = max(0, old_z - new_z) * projected_dual_area
```

Assign removed material to the nearest shoreline point's sediment store.

Cap per-iteration vertical displacement relative to local mean edge length and
coastal-band width. This prevents flipped triangles and makes results less
sensitive to mesh resolution. Several restrained iterations are preferable to
one large destructive update.

After a small batch of iterations, rebuild the shoreline and coastal band so
newly submerged or exposed triangles participate correctly.

Acceptance requirements for Phase D:

- Under equal exposure, a low-hardness synthetic coast retreats materially
  faster than a high-hardness coast.
- No erosion update raises a vertex.
- Per-iteration displacement remains below its geometric cap.
- No generated vertex contains NaN or infinity.
- Triangle XY winding remains unchanged and no zero-area XY faces are created.

### Phase E: Longshore sediment transport

Transport sediment along each ordered shoreline loop with a one-dimensional,
conservative update.

1. Project the weighted mean incoming wave direction onto the local shoreline
   tangent.
2. Use the sign to select the downstream shoreline neighbour.
3. Limit outgoing flux to the material currently stored at that point.
4. Accumulate all transfers into a second scratch array.
5. Swap arrays after the complete loop update.

This must be an explicit conservative transfer; do not update stores in place,
because traversal order would bias the transport direction and lose material.

Where opposing currents converge, retain a larger local load. Where strong
currents leave the simulated coastline or where the equilibrium profile is
full, permit a configured fraction to be exported offshore. Closed island
loops should not lose material merely because their point index wraps from the
last point to zero.

Acceptance requirements for Phase E:

- Transport alone preserves total sediment on a closed synthetic loop.
- Reversing the prevailing wave direction reverses net transport.
- Results are invariant under rotating the starting index of the same loop.
- No sediment store becomes negative.

### Phase F: Beach-profile deposition

Beach formation is strongest where:

- exposure is low or moderate;
- the shoreline is sheltered or concave;
- local terrain is not resistant exposed rock;
- near-shore slopes are gentle enough to retain material; and
- transported sediment is available.

Define a signed shore-normal distance for each coastal-band vertex: positive
inland and negative offshore. Construct a local target profile through sea
level:

```text
inland:   target_z rises gently toward a small berm height
offshore: target_z falls gently toward a closure depth
```

The landward and offshore slopes may differ. Both should vary with exposure:
sheltered coasts receive wider, gentler profiles; exposed coasts receive
narrower profiles and may remain rocky.

Profile shaping has two material-aware parts:

1. If terrain lies above an active low-energy beach profile, cut only the
   permitted excess and add it to the local sediment budget.
2. If terrain lies below the target profile, raise it only as far as available
   sediment volume allows.

Distribute deposition over eligible vertices by deficit volume and a smooth
shoreline influence weight. Apply deltas simultaneously and stop when the
available budget is exhausted.

Deposition must not blindly flatten every coastal point. Hard, high-exposure
headlands should retain cliffs, reefs, and narrow platforms. Broad beaches
should emerge preferentially inside sheltered bays.

Acceptance requirements for Phase F:

- A supplied sediment budget cannot deposit more volume than it contains.
- A sheltered synthetic bay develops a wider and gentler profile than an
  exposed straight coast.
- An exposed hard headland does not acquire a broad artificial beach.
- Depositional updates do not create isolated above-water spikes offshore.
- Eroded, deposited, retained, and exported volume balance within documented
  floating-point tolerance.

### Phase G: Iteration and adaptive coastal tessellation

Before each coastal scale begins, mark faces that:

- cross sea level;
- lie within the active coastal elevation/distance band; or
- are incident to a vertex whose proposed displacement is large relative to
  mean connected-edge length.

Use conforming selective tessellation so shared edges are split once and
neighbouring unsplit faces are stitched. Do not tessellate the entire mesh for
each erosion iteration.

Perform at most one initial coastal refinement and, if the shoreline has moved
outside the refined band, one bounded follow-up refinement. Newly inserted
vertices sample hardness from `GeologyField`; no hardness array needs to be
copied through ancestry.

The coastal process then runs as restrained batches:

```text
extract shoreline
build band and exposure
erode and collect sediment
transport sediment
shape/deposit beaches
apply simultaneous vertex deltas
repeat shoreline extraction after a small batch
```

Iteration counts, maximum displacement, and band width are internal tuning
constants initially. They should not become UI controls until their behavior
is stable.

Acceptance requirements for Phase G:

- Flat inland and deep-sea faces do not gain triangles.
- The shoreline band gains enough resolution to avoid long angular beach
  sections.
- Tessellation remains conforming with no T-junctions or open mesh seams.
- Vertex growth is bounded and reported in benchmark output.

## Pipeline and LOD Integration

Use two coastal scales.

### Coarse coastal evolution

Run the major coastal pass on LOD1 after its hydraulic/thermal shaping and
before the next river-generation stage. This pass creates the lasting bays,
headlands, and broad depositional regions. Subsequent river passes then route
and cut outlets through the evolved shoreline instead of targeting an obsolete
coast.

The exact insertion point should be selected so that:

- the mesh already has sufficient medium-scale relief;
- the broad/final river passes still occur afterward; and
- subsequent LOD0 meshes inherit the evolved LOD1 topology and positions.

### Detailed coastal evolution

Run a weaker, narrower coastal pass on LOD0 after the land-detail river shaping
but before the definitive final river pass. It refines beach profiles, cliff
feet, and small coves. The final river pass then reopens and grades river
outlets after any near-shore sediment deposition.

The detailed pass must not run after the definitive river mesh is duplicated,
or terrain mutation could separate the water mesh from the final terrain.

### River-mouth protection

The final carve-only river run remains the ultimate authority at outlets.
Additionally:

- coastal deposition should reduce eligibility in vertices belonging to a
  known river corridor when that mask is available;
- shoreline sediment may spread beside an outlet but must not create a sill
  higher than the final graded river surface;
- delta terrain built by earlier river stages may feed or interact with the
  beach budget, but the initial implementation should not directly rewrite the
  river sediment accounting; and
- integration tests must trace every final river outlet to connected sea.

### Final LOD correction

Keep `correct_lods` after all terrain and final river mutations. It must copy
the final LOD0 prefix positions into the matching LOD1 and LOD2 vertices, as it
does now. Selective coastal tessellation may append fine-only vertices but must
never reorder or replace existing prefix vertices.

Unity's tile-edge clamping remains responsible only for an active detailed
tile edge facing a coarser neighbour. Coastal evolution must not introduce a
separate seam policy.

## User-Facing Controls

The first version should expose only two new controls:

```rust
pub coastal_erosion_strength: f32,
pub beach_formation_strength: f32,
```

Suggested defaults:

```text
coastal erosion strength = 1.0
beach formation strength = 1.0
```

Suggested Unity slider ranges for initial tuning:

```text
Coastal erosion:  0.0 .. 4.0
Beach formation:  0.0 .. 4.0
```

Zero coastal erosion must bypass coastal tessellation and mutation, preserving
the current generator output. Zero beach formation permits erosional cliffs
and platforms but leaves sediment retained/exported without building beach
profiles.

Do not add Rust runtime range validation for these controls. Unity may constrain
its sliders, while the Rust API accepts caller-selected finite values in line
with the project's current preference for avoiding unnecessary native-side
validation.

The following are useful future controls but are deliberately internal for the
first implementation:

- prevailing wave direction;
- directional wave spread;
- coherent hardness contrast;
- sediment export fraction;
- beach berm height and closure depth; and
- coarse/detail coastal iteration count.

Default wave direction should be derived from the island seed. Hardness must
always remain derived from the initial terrain noise rather than becoming an
independent random slider field.

## API, CLI, Persistence, and Unity Work

### Rust API

Update `IslandOptions` in `src/terrain.rs` with the two fields and defaults.
Keep the fields near `coastal_slope_multiplier` conceptually, but consider
serialization and C layout explicitly before choosing physical field order.

### Native ABI

Update `MotuOptions` and its conversion in `src/ffi.rs`. Update
`include/motu.h` to match exactly. There are no older native callers that need
binary compatibility, but Rust, the header, and Unity must still be changed in
one atomic implementation so field offsets cannot diverge.

### CLI

Add:

```text
--coastal-erosion-strength <S>
--beach-formation-strength <S>
```

Update CLI help and parser tests. Do not impose an artificial upper limit in
the Rust parser.

### Save/load

Bump the island options file format version and append the new values. Continue
loading older saved option files with the new defaults. Add a round-trip test
with non-default values and a compatibility test for the previous format.

### Unity

Update:

- `Assets/Scripts/MotuNative.cs` native option layout;
- `Assets/Scripts/IslandViewer.cs` fields, option construction, sliders, and
  reset-to-default behavior; and
- the Unity README/control documentation if it lists generation options.

Place the sliders near `Coastal slope`, before the hydraulic controls. Display
short explanatory text making clear that erosion forms exposed rocky coast and
beach formation redistributes its sediment into sheltered areas.

After rebuilding and copying `libmotu.dylib`, fully restart Unity before visual
testing because the editor caches loaded native plugins.

## Performance and Allocation Constraints

The coastal stage will run over a high-detail free-form mesh, so it must follow
the project's performance priorities.

Required implementation characteristics:

- Reuse the mesh adjacency and an edge-to-face table for an entire coastal
  stage.
- Restrict most arrays and loops to the active band or shoreline where
  practical.
- Allocate vertex-sized delta, distance, owner, and area buffers once per
  stage and reuse them between iterations.
- Use compact indices (`u32` where consistent with mesh indices) and contiguous
  vectors.
- Never allocate inside wave-ray triangle walking.
- Never create a `HashMap` per shoreline loop, point, or iteration.
- Use edge keys and pre-sized maps only during contour/topology construction.
- Use simultaneous double-buffered updates for erosion, transport, and
  deposition.
- Avoid cloning the full mesh solely to calculate a coastal delta.
- Keep hot loops free of virtual dispatch and unnecessary bounds-independent
  temporary collections.
- Parallelize only after profiling; deterministic accumulation and memory
  locality are more important than premature thread fan-out.

Unsafe indexed access is acceptable in a proven hot path after focused tests
establish all index invariants. Start with clear safe code, benchmark it, and
make unsafe changes only where measurements show material benefit.

Add timing counters for development builds or benchmarks covering:

- contour extraction;
- coastal-band construction;
- exposure tracing;
- erosion/deposition iterations;
- coastal tessellation; and
- total island generation.

Record vertex and triangle counts before and after each coastal scale.

## Validation Strategy

### Unit tests

Add focused tests for:

- shared noise sampling and hardness correlation;
- hardness determinism at arbitrary XY positions;
- sea-level edge interpolation;
- closed contour construction and orientation;
- exact-zero elevation tie handling;
- coastal-band signed distance and nearest-contour ownership;
- projected dual area;
- triangle-walk fetch termination;
- open versus sheltered exposure;
- hardness-dependent erosion;
- conservative longshore transport;
- beach-profile target generation;
- deposition budget limits; and
- complete sediment-volume accounting.

### Integration tests

Add deterministic small-island tests verifying:

- strength zero retains the current baseline output;
- nonzero erosion changes coastline location;
- different coastal strengths produce measurably different shorelines;
- beach formation lowers mean near-shore slope in depositional areas;
- exposed hard coast remains steeper than sheltered depositional coast;
- perimeter vertices remain underwater;
- no disconnected inland seas are introduced;
- final river paths still reach connected sea without an above-surface sill;
- final river meshes remain supported by their terrain beds;
- LOD shared-prefix vertex positions match after final correction;
- grid-sliced terrain retains existing seam tolerances; and
- generated normals, AO maps, heightmaps, and colliders remain finite.

### Property/invariant checks

For a gallery of fixed seeds, collect:

- land area before and after coastal evolution;
- shoreline length;
- shoreline curvature distribution;
- high/low exposure retreat distance;
- beach area and mean beach slope;
- eroded, deposited, retained, and exported sediment volumes;
- vertex and triangle counts; and
- generation duration.

Set broad regression bounds rather than requiring identical floating-point
statistics across all platforms.

### Visual evaluation

Generate the same seed gallery with:

1. coastal erosion and beach formation disabled;
2. default values;
3. strong erosion with default beach formation;
4. default erosion with strong beach formation; and
5. strong erosion with beaches disabled.

Inspect in both orbit and first-person Unity modes with mesh edges optionally
enabled. Evaluation should cover:

- recognizable coherent headlands rather than uniform shoreline noise;
- bays aligned with softer portions of the original terrain field;
- broad sheltered beaches and narrow exposed coast;
- no radial hardness pattern from the island falloff;
- no offshore spikes, inverted faces, floating shelves, or holes;
- no river-mouth dams;
- no visible LOD tile seams; and
- collider agreement with the visible terrain.

Visual acceptance requires several seeds. A single attractive island is not
sufficient evidence that the process is stable.

## Implementation Phases

### Phase 0: Baseline and instrumentation

- Select a fixed seed gallery and record current mesh/shoreline statistics.
- Save baseline renders for zero-strength comparison.
- Add lightweight helpers for land area and shoreline metrics in tests.

Exit criteria:

- Baseline images and numeric metrics exist for repeatable comparison.
- Existing Rust checks and seam diagnostics pass before implementation.

### Phase 1: Shared terrain/geology field

- Extract shared continental/detail sampling.
- Add calibrated `GeologyField` hardness sampling.
- Prove zero-strength terrain equivalence.

Exit criteria:

- Phase A acceptance requirements pass.
- No user-facing option or mesh result changes yet.

### Phase 2: Shoreline topology and coastal band

- Add `src/coast.rs`.
- Implement contour extraction, loop construction, orientation, tangents,
  curvature, signed band distance, and dual areas.
- Add synthetic topology tests.

Exit criteria:

- Phase B acceptance requirements pass.
- Utilities handle all fixed-seed island coastlines without malformed loops.

### Phase 3: Exposure and erosional coast

- Implement face adjacency suitable for triangle ray walking.
- Implement deterministic wave climate and fetch/exposure.
- Implement hardness-weighted wave attack and platform cutting.
- Add erosion volume tracking.

Exit criteria:

- Phases C and D acceptance requirements pass.
- Default test seeds develop differential retreat without invalid geometry.

### Phase 4: Sediment transport and beaches

- Implement conservative longshore transport.
- Implement exposure-dependent equilibrium beach profiles.
- Deposit only within available material budgets.
- Add complete accounting metrics and tests.

Exit criteria:

- Phases E and F acceptance requirements pass.
- Sheltered beaches form without covering exposed headlands uniformly.

### Phase 5: Adaptive refinement and pipeline integration

- Add coastal-band selective tessellation.
- Insert coarse and detail passes at the agreed pipeline points.
- Ensure later river passes reopen outlets.
- Preserve prefix indexing and final LOD correction.

Exit criteria:

- Phase G and LOD integration acceptance requirements pass.
- Existing river, waterfall, seam, slicing, and collider tests remain green.

### Phase 6: Controls and Unity integration

- Add the two Rust options and defaults.
- Update save/load, CLI, C header, C ABI, and Unity native struct.
- Add Unity sliders and reset behavior.
- Rebuild and install the macOS native plugin.

Exit criteria:

- Native struct layouts match.
- CLI and save/load tests pass.
- Unity regenerates islands across the full slider ranges after a full restart.

### Phase 7: Profiling and visual tuning

- Measure allocation counts, stage timings, and mesh growth.
- Optimize only measured hot paths.
- Tune defaults against the multi-seed gallery.
- Document final behavior and controls in both READMEs.

Exit criteria:

- Coastal generation cost and mesh growth are recorded and considered
  acceptable relative to total island generation.
- Default settings meet visual acceptance across the seed gallery.
- Full validation below passes.

## Final Validation Commands

From `island-rs`:

```sh
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Also run the focused generated-island tests and seam diagnostic separately so
their status can be reported distinctly if the full suite is slow.

After the release build:

1. Copy the rebuilt platform library into the Unity plugin folder.
2. Verify the copied library matches the release artifact byte-for-byte.
3. Confirm the macOS plugin architecture is arm64.
4. Fully quit and restart Unity.
5. Regenerate the fixed seed gallery and exercise the new sliders.
6. Test orbit view, first-person entry, tile streaming, collider changes, LOD
   transitions, mesh-edge display, normal maps, and AO maps.

## Overall Acceptance Criteria

The feature is complete when all of the following are true:

1. Hardness is deterministic, coherent, and derived from the same continental
   and detail noise used for initial terrain height, excluding radial falloff.
2. With coastal erosion disabled, existing generation output is preserved.
3. Default generation creates persistent hard headlands, softer eroded bays,
   and preferentially sheltered beaches across multiple seeds.
4. Coastal sediment is volume-accounted and deposition cannot exceed available
   material.
5. Coast evolution uses the free-form mesh and conforming selective
   tessellation, not a grid-defined terrain.
6. Exposed hard coast remains meaningfully different from sheltered soft coast;
   the system does not converge toward a uniformly smooth circular island.
7. Final rivers reach the sea and coastal deposition does not dam their mouths.
8. River water, terrain beds, waterfall geometry, and beach terrain remain
   mutually supported with no floating shelves or spikes.
9. LOD shared vertices are corrected to final LOD0 positions and streamed tile
   seams remain within the existing tolerance.
10. Unity exposes coastal erosion and beach formation sliders and can regenerate
    safely throughout their ranges.
11. The release native library is rebuilt and installed, and Unity visual
    verification is performed after a full editor restart.
12. Formatting, tests, strict Clippy, release build, focused seam tests, and the
    multi-seed visual review all pass or have separately documented blockers.

## Explicit Non-Goals for the First Version

- Real-time coastal erosion inside Unity.
- Tides, sea-level change, or seasonal storm simulation.
- Fully three-dimensional cave, arch, or sea-stack fracture simulation.
- A raster or voxel replacement for the free-form terrain mesh.
- Independent user-painted geology maps.
- Direct coupling of river sediment mass into the coastal budget before the
  basic coastal process is stable.
- Exposing every physical constant as a slider.

These can be revisited after the deterministic offline generator produces
stable headlands, bays, beaches, river outlets, and LOD-compatible meshes.

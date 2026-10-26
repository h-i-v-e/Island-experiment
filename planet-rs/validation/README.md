# Planet geometry acceptance

The Rust tests run without a renderer:

```sh
cargo test --workspace --locked
```

They cover regular solids, exact cocircular sites, equal-projection events,
close sites, nearly coplanar events, invalid options and input, 100 random
seeds, multiple sweep directions, deliberate mesh damage, and an exhaustive
supporting-plane reference on a small random set.

For an independent reference, build the viewer and run `compare_qhull.py` with
a Python environment containing NumPy and SciPy:

```sh
cargo build -p planet-bevy --locked
python planet-rs/validation/compare_qhull.py --viewer target/debug/planet-bevy
```

The script asks the executable to generate and export meshes without a GPU or
window, then compares every triangle to `scipy.spatial.ConvexHull` (Qhull).
SciPy is an optional validation dependency, not part of the Rust application.
Its temporary OBJ files are cleaned up automatically.

On 22 October 2026, all 28 cases matched: seeds 0, 42, and 123 at 32, 256,
and 1024 vertices, each swept along X, Y, and Z, plus 8192 vertices at an
export radius of 6,371,000. The upper-bound case completed in about 1.57 s,
including process startup, full validation, and OBJ export on this machine.
This initial quadratic algorithm is intended for modest point counts.

The Bevy screenshot path renders the actual finished mesh offscreen:

```sh
cargo run -p planet-bevy -- --sphere-only --wireframe --screenshot planet.png
```

The output path must be a new PNG. Interactive orbit, zoom, edge display,
and shading toggles are separate from geometric validation.

## Radial terrain acceptance

The default viewer now runs the terrain pipeline. The Qhull comparison script
passes `--sphere-only` explicitly because radial displacement may make terrain
concave; spherical Delaunay validation applies to the original directions and
connectivity, not to the displaced surface's convex hull.

The terrain tests cover 20 seeds, scale invariance with respect to sea radius,
Cartesian noise continuity across poles and the longitude seam, positive radial
positions, outward normals, area-weighted erosion transport, real downhill
drainage through local depressions, and river paths ending below sea level.
They explicitly allow a radially descending river to increase world Y.
Flat worlds above, below, and exactly at sea level preserve the original radius
and do not acquire artificial perimeter oceans or outlets.

Render both hemispheres and expose the seabed with:

```sh
cargo run -p planet-bevy -- --screenshot terrain-front.png
cargo run -p planet-bevy -- --view back --screenshot terrain-back.png
cargo run -p planet-bevy -- --no-ocean --wireframe --screenshot terrain-bed.png
```

The CPU stages adapt the island sequence with curvature-selected tessellation,
orientation-safe tangent-plane relaxation, radial smoothing, and sequential
hydraulic paths. Unlike the planar generator, hydraulic excavation remains radial. Water surface
animation, vegetation, caves, and LOD
are not part of this acceptance milestone. Final river incision records its
removed material as exported sediment rather than depositing it as a delta.


## Tessellation and final channel acceptance

The refinement tests check conforming partial/all-edge subdivision, sphere
closure, constant-field interpolation, conservative smoothing, and no height
overshoot. Positional relaxation additionally checks sphere closure, actual site
movement, and conservation of height/soil budgets after control areas change.
Adaptive-detail tests distinguish relief curvature from the sphere itself.
Hydraulic-only tests disable thermal erosion and all river stages to prove
excavation and deposition change actual elevations and balance their volumes;
zero hydraulic strength leaves that fixture unchanged. Channel tests check dense corridor geometry, actual radial bed cuts,
positive centreline water clearance, and water triangles lying above their
supporting final terrain face. The retained channel-depth field reconstructs
the pre-carve heights without duplicating the whole mesh.

```sh
cargo run -p planet-bevy -- --view river --screenshot river-water.png
cargo run -p planet-bevy -- --view river --no-rivers --no-ocean --screenshot river-bed.png
cargo run -p planet-bevy -- --view river --no-rivers --no-ocean --uncarved --screenshot river-before.png
```

All three use the same deterministic terrain and camera. The interactive B
key toggles original/carved beds, and R toggles the generated water mesh.


## Global ocean acceptance

The sea-connection tests verify nearest-sea selection and continuation from the
sea just reached (rather than restarting at the original sea), actual excavation
on a spherical mesh, balanced removal diagnostics, idempotence, and no changes
to dry/fully submerged/sea-level spheres. Integration checks exercise LOD2,
and LOD1, require exactly those two connection passes, allow later disconnections,
and retain the separate seas comparison mode. Existing multi-seed, radius scaling, river drainage, and
water-above-bed checks run with global-ocean generation enabled.

```sh
cargo run -p planet-bevy -- --validate-only
cargo run -p planet-bevy -- --screenshot global-ocean.png
cargo run -p planet-bevy -- --separate-seas --screenshot separate-seas.png
```


## Interactive parameter panel

Viewer tests exercise exact u64 seed editing, combined-parameter validation,
background generation with unchanged current terrain until completion, edits
made while a request is in flight, failed-job preservation, and ownership-moving
Previous comparisons. Visual checks cover the side-panel layout, independent
world viewport, pointer/keyboard capture, display toggles, and regeneration.
Use `--hide-controls --screenshot` for a clean planet capture; leave controls
visible to capture the editor itself.

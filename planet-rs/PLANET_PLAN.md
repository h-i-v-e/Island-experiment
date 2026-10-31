# Planet experiment: conversion plan

Created: 22 October 2026.

## Goal and current state

Develop a procedural planet generator alongside the Island applications. Start
with seeded random vertices on the surface of a sphere. Move a plane through
the sphere to order an incremental 3D convex-hull construction; its boundary
triangles form the spherical Delaunay triangulation.

The initial target is Rust/Bevy. Unity conversion is a later decision. This
commit history is inherited from Island-experiment at
`e12098d87dfbe63050aed4ce3bbb9f3d00aaf925`. The prototype now lives in `planet-rs` and `planet-bevy` in the
Island-experiment repository, with its own root Cargo workspace. The Island
crates retain their independent workspaces and build configurations. The Planet
crates were moved back from the separate fork on 27 October 2026.

## First milestone: a plain, closed sphere

Inputs: a deterministic seed, vertex count, and positive radius. Store stable
vertex IDs, unit directions, indexed outward-facing triangles, and adjacency.
Use f64 for geometric construction and predicates, converting to the viewer's
rendering representation at the boundary. f64 alone is not a robustness
guarantee: use filtered/adaptive orientation predicates near degeneracies.

Generate points uniformly by sphere surface area. For independent uniform
values u and v in [0, 1), set z = 1 - 2u, phi = 2*pi*v, and direction =
`(sqrt(1-z*z)*cos(phi), sqrt(1-z*z)*sin(phi), z)`. Scale by radius only after
constructing topology. Uniform latitude sampling would cluster points near
the poles. Reject coincident or numerically indistinguishable sites using a
documented angular separation rule; keep sampling deterministic.

The initial mesh is a triangulated sphere before elevation, erosion, water,
vegetation, atmosphere, or LOD work. Reject unsupported inputs clearly,
including fewer than four usable non-coplanar points and point sets whose
convex hull does not strictly contain the origin. Seeded resampling for such
sets can be considered explicitly; do not silently claim whole-sphere coverage.

## Selected construction: a plane translating through the sphere

The user confirmed Rust/Bevy and delegated the geometry choice, suggesting a
plane moving through the sphere in any direction. Use that simpler translation
as the insertion order for one global incremental convex hull. The plane
controls when vertices enter; hull visibility and adjacency determine the
triangles. This is not a succession of independently triangulated slices.

For a unit sweep direction n, the plane is `dot(n, p) = t`. Sort the original
unit-sphere vertices by `dot(n, p)`, breaking ties by stable vertex ID, and
advance t from below the minimum projection to above the maximum. Default to
a fixed axis for reproducibility; allow other directions for inspection and
direction-invariance tests.

1. From a prefix of the sorted sites, construct a nondegenerate tetrahedron
   deterministically. Insert any other sites from that prefix before resuming
   the ordered sweep. Reject a fully coplanar input. Retain an interior point
   of the initial tetrahedron to orient faces consistently during construction;
   the origin may lie outside early partial hulls.
2. For each entering vertex, locate the hull faces visible from it using robust
   3D orientation predicates. Remove those faces and extract the horizon edges
   between removed and retained faces.
3. Join the horizon to the new vertex and retain global vertex IDs. The initial
   implementation derives the horizon from opposite edge cancellation, without
   maintaining a separate mutable adjacency cache. Preserve a valid closed hull
   after each insertion; handle exact
   coplanar events with a deterministic policy. Report unexpected loss of an
   accepted sphere site as a geometry error.
4. When every site has entered, check that the origin is strictly inside the
   hull and validate the completed spherical mesh. The back of an intermediate
   hull closes its processed point set; it is not yet the finished planet.

For points on a sphere covering all directions, the boundary triangles of
their 3D convex hull give the spherical Delaunay triangulation. Use a separate
trusted hull implementation as a small-input correctness oracle; select its
dependency and license before adoption.

Begin with a simple visible-face scan at modest vertex counts. Ordering by a
plane does not itself guarantee an efficient hull algorithm: this baseline can
take quadratic time. Measure it, then use adjacency walks and conflict tracking
if needed. If generation scale makes the sweep expensive, compare a proven
Quickhull implementation while retaining the same spherical mesh contract.

### Geometric contract

For an outward-oriented face (a, b, c), all other unit-sphere sites must lie
on or behind its supporting plane, within a documented robust predicate
policy. Equivalently, its outward spherical circumcap is empty. Handle exact
cocircular ties deterministically; valid tied triangulations may differ from
the oracle's triangle list, so compare geometry and invariants as well.

Background references:

- [CGAL: triangulations on the sphere](https://doc.cgal.org/latest/Triangulation_on_sphere_2/index.html)
  explains the spherical empty-circle test, orientation predicates, numerical
  separation, and incomplete hemisphere coverage.
- [Qhull: Delaunay triangulation](https://www.qhull.org/html/qdelaun.htm)
  describes convex hull construction for sites on a sphere.
- [CGAL: 3D convex hulls](https://doc.cgal.org/latest/Convex_hull_3/index.html)
  describes hull construction, Quickhull, and numerical predicate requirements.

## Verification and first visible result

For a valid connected triangular sphere with V retained vertices, require:

- Every input site is represented exactly once; all indices and coordinates
  are valid and finite, with every vertex at the requested radius.
- Distinct nondegenerate triangles, consistently wound outward.
- Every undirected edge has exactly two incident faces with opposite edge
  directions. Each vertex link is a single cycle and the mesh is connected.
- No intersecting nonadjacent surface triangles and no unused vertices.
- Euler characteristic V - E + F = 2, with E = 3V - 6 and F = 2V - 4.
- Spherical empty-cap checks and agreement with a convex-hull reference on
  nondegenerate small fixtures. Validate tied cases by invariants.
- Deterministic output for repeated seeds and explicit coverage of poles,
  equal-projection events, first and last insertions, close pairs, duplicates,
  cocircular sites, and invalid inputs. Different sweep directions must produce
  the same triangle set on nondegenerate sites; tied cases must satisfy the
  same geometric and topological contract.

Render the finished mesh in Bevy with an orbit camera, flat shading, and an
optional triangle-edge overlay. The user requested the finished mesh only;
plane/frontier animation and point IDs are deferred. Expose seed, vertex count,
radius, and sweep axis through the CLI. Report validation errors before opening
the viewer instead of presenting an incomplete mesh as successful. Benchmark
construction across increasing counts after correctness is established.

## Integration with the inherited generator

The current `island-rs/src/terrain/generation.rs::generate_base` creates planar
seed points and calls `Mesh::delaunay(&[Vec2])` in `island-rs/src/mesh.rs` before
assigning elevation and running erosion. Add an explicit planet generation
entry point rather than silently changing this planar method's contract.

Keep geometry independent of rendering so tests do not require a GPU or Unity.
The first viewing path is a small `planet-bevy` binary using the inherited
Bevy version and offscreen capture approach, without loading island-specific
sea, rivers, vegetation, collision, or terrain stages. `planet-rs` uses glam
and the MIT/Apache-2.0 `robust` crate's adaptive 3D orientation predicates.
The owned generator mesh retains shared indexed vertices; flat shading expands
only the render buffers. No geometry cache is used in this first prototype.
Keep inherited crate and package names until their planet adaptations are
clear. Introduce a separate versioned cache namespace when caching is added.

## Radial terrain milestone implemented

The original sphere radius is fixed sea level. The planet pipeline now samples
Cartesian 3D continental/detail noise, assigns bedrock hardness and loose soil,
transports sediment through hydraulic erosion, applies thermal relaxation, and
routes/carves rivers before rebuilding normals. Surface distances are angular,
control areas are spherical solid angles, and down is toward the centre.

The inherited island's radial edge falloff, perimeter ocean connectivity,
water-ratio thresholding, coast-distance height remapping, inland-water removal,
and sea exclusion masks are not applied. Any submerged site can receive a river.
Dry pits are connected by a sea-seeded priority flood and downhill incision;
all-dry worlds remain valid without manufactured outlets or a changed sea level.
After initial terrain/erosion, each impact batch, each refinement stage, and
final terrain smoothing/erosion, retain the largest submerged component by
spherical surface area as the global ocean. Fill all other seas to at least
0.5 physical metres above the original sea radius, recording the added loose
material separately from incision. There are no ocean-connecting channel cuts.
The final fill occurs before river profiling and final river-bed carving, so
rivers can cut through the raised basins without being plugged afterward.
All-dry worlds remain dry; single seas stay unchanged. `--keep-inland-seas`
(alias `--separate-seas`) disables filling for comparison.

See [the routing algorithm background](https://arxiv.org/abs/1511.04463).

The viewer displays the finished terrain with a separate sea sphere at the
original radius and water meshes fitted to the final carved terrain. Smooth
normals, elevation/slope/material colours, opposite-hemisphere capture, and
sea/river visibility controls support inspection. Two broad conforming subdivisions followed by curvature-selected refinements,
tangent-plane positional relaxation and conservative radial smoothing, and
sequential momentum/sediment erosion stages are followed by up to three width-targeted
river-corridor subdivisions with a two-ring apron, separate waterfall-patch
and final-face detail tiers,
Island's stepped river profiles and waterfall patches in local tangent frames,
flow-scaled cross-section carving, downstream drainage repair, and water mesh
construction from the fixed pre-carve profile. Island's low-bank lift and blended
outer apron support the water; final boundary vertices meet the actual terrain
after drainage repair. Four final radial relaxation passes smooth the corridor
and outer apron while holding the banks and hydraulic patch. Two simultaneous
tangent-plane outline passes round shared bank vertices with the original
movement limits and hydraulic/junction anchors. Protected waterfall support
retains a small clearance. An upstream distance weight blends terrain lift/clamp
into the lip, with six weighted radial smoothing passes. Final relaxation uses the original two groups
of six passes along the whole river corridor and bank apron. Full-strength
smoothing extends through two outer mesh rings and fades over two more, as
in Island, to prevent a raised edge at the smoothing boundary. Waterfall detail
tessellation stays local; the support mask controls initial terrain shaping, so the
smooth has no longitudinal cutoff at the lip or foot. Ordinary bank floors,
hydraulic ceilings and minimal lifts of connected feeders preserve pinning and
downhill paths. Submerged roots and seabed heights stay fixed. Sea-level channel interiors
use an opening mask separate from visible river core and dry banks. Carve the
terminal bed to the channel-depth cross-section and preserve it through every
smoothing/lift stage. Final submerged roots cannot be lifted by bank grading;
assert that no masked opening has returned above sea level.
The 2.5-metre shaping run and upstream repair apron stay intact. At placement,
use the incoming tangent and a straight, constant-width cross-section frame
through the drop. Blend centreline distance and width into the natural reaches
through both shoulders and the outer bank apron. Natural-reach ownership avoids
capturing nearby disconnected channels. No post-shaping terrain compression is
applied. Frozen hydraulic stations retain river order during final bank grading.
Final water height blends the terrain + 3 cm method and the regular hydraulic
method using the smoothstep of `clamp(dot(terrain_normal, radial_up), 0, 1)`
throughout the river, easing gently into each endpoint method.
The normal field comes from the final terrain; no face/foot mask switches the
water method. Dry boundary vertices stay terrain-clamped at every inclination.
Finally, grade actual bank-outline edges downstream to upstream using distance
along their owning connected reaches. Lift upstream dips on each side separately
and exclude submerged openings. A sea-seeded lift-only flood respects that bank
ordering, raises newly trapped feeders and rebuilds downhill rainfall routing.
No smoothing or incision follows this bank pass; water is pinned afterwards.
Grade both the initial corridor and post-waterfall wet outlines. Use common
outlet ownership across short reaches, extend longitudinal coordinates through
endpoint caps, and repeat if the final water field exposes additional banks.
Validate that the retained upstream bank constraints have no remaining dips.
Bank edge vertices are shared across adjacent faces and confluences, and validation
rejects exposed floating edges. Net bank lift is tracked separately from incision. The river close-up and B toggle compare pre-carve
and post-carve radial heights on the final topology. Water animation remains separate from these CPU stages.

## Later milestones

The tectonic source also supports irregular continental fragments carried by
plate motion and branching subsiding rift basins. Fragmentation and fragment
scale control how strongly crust departs from whole plate cells; rift strength
and branching levels control the additional fault-basin network. Zero
fragmentation restores the original plate-cell crust, while zero rift strength
or levels disables the new basins without removing existing boundary extension.
These source fields are sampled before erosion and through terrain refinement;
river/waterfall profiles retain their existing later ordering. Mountain belts,
side ridges, volcanoes and impact craters remain supported, sea level is the
original sphere, and inland-sea filling continues through terrain generation
before final river carving.
The viewer exposes all four controls in Tectonic plates, persists them with
other settings, and supports matching `--tectonic-fragmentation`,
`--tectonic-fragment-scale`, `--tectonic-rifts`, and `--tectonic-rift-branches`
CLI options. Drowned-valley and marine sediment transport models remain future
coastal-generation work.

1. Completed: deterministic elevation evaluated in 3D direction space, displacing
   vertices radially as `(radius + height(direction)) * direction`. Recompute
   normals and audit material sampling for sphere seams and pole distortion.
2. Completed for the CPU terrain prototype: erosion/drainage use spherical
   adjacency, angular slopes, solid-angle areas, and radial heights; sea remains
   at the original radius. Final river cross sections and bank-fitted surface
   meshes are implemented. Sediment export deposition remains future work.
3. Adapt vegetation, rocks, collision, player movement, and lighting to local
   tangent frames and radial up.
4. Implemented for render geometry: icosahedral cells, exact shared boundaries,
   four LODs with shared borders fixed, conservative height-aware horizon/frustum rejection,
   asynchronous cached detail loading, coarse fallbacks and GPU detail budgets.
   LOD1–3 carry independent normal/AO atlases sampled from global finest terrain,
   using physical local tangent AO probes, padded UV charts and bounded mips.
   Atlas bytes are cached and streamed with geometry. Coarse terrain releases
   interior hydraulic pins and bakes river coverage into a one-byte texture;
   separate fitted river meshes render only alongside actual LOD0 terrain.
   LOD0 generation, hydrology and collision retain the full source geometry. Construction/clipping use f64; GPU buffers use normalized f32 positions.
   Global generation/source terrain and exact CPU collision remain resident.
5. Review atmosphere, clouds, shadows, ocean rendering, and eventual Unity
   integration against the new planetary coordinate system.

## Prototype acceptance

The prototype must pass the sphere invariants, deterministic and multi-axis
fixtures, degeneracy/rejection cases, and an independent convex-hull comparison.
Capture and visually inspect the actual Bevy result. Leave source changes
uncommitted for review. Terrain acceptance also covers multiple seeds, radial
radius scaling, downhill routes reaching water, all-dry/all-wet worlds, and
seam/pole continuity of the 3D noise.


## Upstream river narrowing

A shared per-vertex width field combines flow growth with represented river-path
distance, narrows headwaters and tributaries and preserves mouth widths. Reach
endpoints interpolate that field continuously through shared junctions. Before
waterfall profiling and river tessellation, a radial port of Island's channel
ring pull brings bank vertices toward the centreline with a per-face orientation
guard. Centreline, merge rings, submerged vertices and radial heights stay fixed.
The frozen widths also size refinement targets and waterfall/support footprints;
existing downstream carving, bank pinning and upstream contact grading still run
afterward. `--river-taper 0` restores the previous width/geometry rule; 1 is the
default. The viewer exposes the remembered control as **Upstream narrowing**.

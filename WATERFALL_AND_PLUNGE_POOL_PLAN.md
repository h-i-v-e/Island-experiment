# Straight Waterfall and Plunge Pool Plan

## Status

Implemented on 2026-08-19 in `island-rs/src/rivers.rs`.

The implementation uses one spatially bounded 1.25-metre waterfall refinement
pass, full-width pinned upper and lower terrain constraints, a steep
height-field-safe support surface, an explicit render-only vertical curtain,
and an eligibility-gated elliptical plunge pool with a downstream outlet.
Waterfall constraints remain protected from the ordinary final channel
smoothing and local edge optimization.

A follow-up correction on 2026-08-19 keeps the stable terrain and ordinary
river topology intact, but prevents waterfall-support water vertices from
riding above the terrain. They instead use the lower water elevation. An
experimental recessed terrain curve, removal of lip-crossing water triangles,
and support-edge retriangulation were reverted after producing holes and large
vertical facets in Unity.

A second follow-up adds a final four-pass, height-only relaxation after channel
clearance. Tessellation adds resolution but preserves the original planar faces
unless the resulting vertices are subsequently reshaped. The first version
only averaged downward and therefore left planar faces largely unchanged; it
also incorrectly applied the ordinary lower-water ceiling to waterfall support
vertices, which could collapse the steep support into a hole.

The corrected relaxation propagates target channel depth through tessellation
alongside surface, lateral coordinate, and target width. It evaluates a smooth
cross-channel height profile at every refined vertex and combines that profile
with bidirectional neighbour smoothing. Waterfall support instead retains its
analytic upper-to-lower target and is exempt from the lower-water ceiling in
both the preceding channel-clearance pass and the final relaxation. The outer
seam, river banks, waterfall lips, and lower landings remain fixed.

A third follow-up enforces a final longitudinal grade invariant after width
compensation, because compensation previously ran after waterfall placement
and could turn a flat water terrace into a sharply descending bed. Ordinary
reaches may now deepen at no more than a 4 percent floor grade. A water-surface
segment exceeding that grade is explicitly converted into a waterfall instead.
At each waterfall, every covered river vertex within the patch is assigned the
same upper or lower terrace, preventing the outer channel and banks from
forming diagonal ramps around an otherwise vertical curtain.

The original implementation passed focused river and mesh suites plus a short
complete-island clearance smoke test. The first follow-up passed all 46 river
tests, strict Clippy, formatting, and the release library build. The final
corrected relaxation and longitudinal grade enforcement add focused
cross-channel, waterfall-support, and gentle-grade regressions and pass all 49
river tests, strict Clippy, formatting, and the release build. The rebuilt
macOS Unity plugin is installed with a matching source-artifact checksum. The
slow full-island smoke is intentionally not repeated.

The straight waterfall is the required outcome. The plunge pool is an optional,
separately gated enhancement and must not delay or destabilize the waterfall
fix.

## Goal

Replace failed waterfall transitions that collapse into a large triangular fan
with a deliberately constructed waterfall:

1. a stable upper river terrace ending at a clearly defined lip;
2. a visually straight water curtain;
3. a broad lower landing rather than one low centre vertex;
4. a steep but valid terrain surface hidden behind the curtain; and
5. where the surrounding topology permits it, a smoothly carved plunge pool
   that drains into the downstream river.

## Current problem

The river profile already identifies waterfall segments, but final terrain and
water construction do not treat the complete drop as one constrained feature.

- General river refinement reduces triangle size but continues to interpolate
  ordinary terrain and river attributes across the drop.
- The upper waterfall lip is marked, but the lower landing is not represented
  as a corresponding cross-channel ring.
- The final channel-clearance pass can lower the downstream centre and its
  immediate neighbours without constructing a complete waterfall landing.
- The extracted river-water mesh performs one additional lip tessellation, but
  it smooths only vertices that retain the upper-lip flag. It does not shape the
  whole curtain.

This permits the upper terrace to converge onto a single low vertex, producing
the large fan-shaped face visible in the mesh overlay.

## Geometry constraints

### Terrain and collider

The terrain mesh and hidden Unity `TerrainCollider` are height fields: they have
one height for each horizontal position. They cannot represent a true vertical
wall, an overhang, or two terrain vertices with identical XY coordinates.

The supporting ground must therefore remain a very steep, densely sampled
slope with a small horizontal run. It may be hidden by the water curtain and
cliff material, but its projected triangles must remain valid.

### Visible waterfall

The river-water mesh is not restricted to a height field. A separate curtain
strip can use duplicated upper and lower XY positions, allowing the visible
water to fall vertically.

The curtain must be appended after the normal river topology has been
duplicated. The existing XY deduplication used by ordinary river surfaces would
otherwise merge the upper and lower curtain vertices.

## Proposed data model

Create one derived `WaterfallPatch` for every accepted waterfall segment. It
should contain enough information to shape terrain and water without repeatedly
inferring the relationship from ambiguous mesh rings:

- river and segment indices;
- upstream and downstream centreline nodes;
- downstream direction and cross-channel direction;
- upper and lower water elevations;
- upper and lower channel-floor elevations;
- local target half-width;
- upper-lip vertices;
- lower-landing vertices;
- supporting terrain and bank-apron masks;
- waterfall-curtain vertices or sampling positions;
- optional plunge-pool eligibility and dimensions.

The patch is transient generation data. It does not need to cross the C ABI.

## Target pipeline

1. Establish river profiles and accepted waterfall segments.
2. Shape normal river widths and carve the ordinary corridor.
3. Derive waterfall patches from the final river path and target widths.
4. Perform waterfall-specific terrain refinement.
5. Construct and constrain the upper lip, supporting slope, and lower landing.
6. Optionally carve an eligible plunge pool.
7. Apply constrained local smoothing and protected-edge triangulation.
8. Run final river clearance while excluding pinned waterfall constraints.
9. Extract the ordinary upper and lower river-water surfaces.
10. Append explicit waterfall curtain geometry.
11. Calculate final normals, UVs, riverbed masks, LOD slices, and collider
    heightmaps.

## Phase 1: Waterfall diagnostics and invariants

Before changing geometry, add focused diagnostics that identify malformed
waterfalls deterministically.

- Record the river index, segment index, drop height, target width, lip width,
  landing width, longest patch edge, and smallest projected triangle area.
- Detect a failed landing when the downstream drop converges onto fewer than
  the expected cross-channel vertices or is substantially narrower than the
  local river width.
- Detect terrain faces spanning a waterfall whose edge length exceeds the
  waterfall-specific target.
- Add a focused synthetic test reproducing an upper terrace collapsing onto one
  downstream centre vertex.

This phase should make it possible to distinguish a waterfall defect from a
normal bank or river-corridor defect without relying solely on screenshots.

## Phase 2: Derive upper and lower waterfall rings

For each waterfall segment:

1. Treat the marked node as the upper lip and the following node as the lower
   landing centre.
2. Use the existing channel owner, flow direction, and target half-width to
   collect or project a complete cross-channel upper ring.
3. Construct a corresponding lower ring at least as wide as the downstream
   channel target.
4. Give both rings stable local coordinates: downstream distance and signed
   lateral distance.
5. Resolve overlaps deterministically at confluences or nearby waterfalls by
   preserving the earlier accepted feature and shortening or rejecting the
   conflicting patch.

The lower ring is essential. Tessellating a face that still terminates at one
low point would only produce a denser triangular fan.

## Phase 3: Waterfall-specific terrain refinement

Apply one additional selective tessellation tier to:

- triangles touching the upper lip;
- triangles between the upper and lower rings;
- triangles touching the lower landing; and
- one surrounding bank ring needed for blending.

Use a stricter maximum horizontal edge length than the ordinary river corridor,
initially targeting approximately 0.75 to 1.5 metres. Cap the pass count and
limit it to the waterfall patch so generation cost remains bounded.

For every new midpoint:

- extend surface material, river coverage, surface height, UV, target width,
  and waterfall ownership using the existing tessellation stencils;
- derive waterfall position from the patch's local coordinates rather than
  blindly averaging upper and lower terrace heights; and
- retain explicit upper-edge, curtain, lower-edge, and bank-apron roles.

## Phase 4: Shape the supporting terrain

The terrain beneath the curtain should be steep, stable, and unobtrusive.

1. Pin the upper terrain ring to the upstream channel floor.
2. Pin the lower terrain ring to the downstream landing floor.
3. Give the supporting slope a small nonzero downstream run so it remains a
   valid height field and can be represented by the TerrainCollider.
4. Interpolate intermediate terrain heights with a monotonic profile; do not
   allow a midpoint to rise above the upper floor or fall below the lower
   floor.
5. Blend side-bank heights laterally into the surrounding terrain without
   changing the pinned upper and lower edges.
6. Remove or redistribute loose soil consistently with existing river erosion
   and sediment accounting.

The supporting terrain is not expected to look vertical with the water hidden.
Its purpose is collision stability and removal of gaps behind the curtain.

## Phase 5: Constrained waterfall smoothing

Run two small, snapshot-based height-smoothing passes over the refined patch.

- Never move the upper lip or lower landing vertically.
- Never smooth across the upper or lower constrained edge.
- Smooth curtain-support vertices primarily across the channel, not along the
  drop.
- Smooth the bank apron outward with a metric-distance falloff.
- Reject any movement that would invert a projected terrain triangle.
- Protect lip and landing edges during the final local surface triangulation.

The ordinary final channel-integrity pass must recognize waterfall patch roles:

- upper-lip vertices remain pinned;
- curtain-support vertices are excluded from ordinary centreline smoothing;
- lower-landing vertices remain at the calculated downstream floor; and
- the downstream channel resumes from the complete landing ring.

## Phase 6: Build the visible straight water curtain

Split the normal river-water surface at each waterfall.

1. End the upper river surface exactly at the upper lip.
2. Begin the lower river surface at the lower landing.
3. Append a separate curtain strip connecting corresponding lateral samples of
   the two rings.
4. Duplicate upper and lower curtain vertices even where their XY coordinates
   match, allowing a truly vertical visible face.
5. Use consistent triangle winding and explicit curtain normals.
6. Preserve downstream distance in the V coordinate and use lateral position
   or normalized width in U so the river shader remains stable.
7. Add enough lateral subdivisions to avoid a single wide quad, but do not
   smooth the upper or lower edge away.
8. Keep the existing hard sea-level clipping rule. A curtain must never be
   emitted below the visible end of the river mesh.

This phase provides the straight visual drop. The terrain behind it remains the
collider-safe approximation described above.

## Phase 7: Optional plunge pool

The plunge pool should be attempted only after the straight waterfall and broad
landing are stable.

### Eligibility gate

Skip the pool and retain the ordinary broad landing if any of these apply:

- the landing is too close to the terrain perimeter or sea transition;
- another waterfall, confluence, or protected river feature overlaps the
  proposed basin;
- too few terrain vertices can be refined within the bounded pass count;
- the basin would cut below an applicable terrain or collider safety limit;
- a valid downstream outlet cannot be maintained; or
- projected triangle validity cannot be preserved.

Skipping a pool is an expected fallback, not a generation failure.

### Pool shape

For an eligible waterfall:

1. Centre an elliptical basin slightly downstream of the curtain.
2. Align its long axis with river flow and size it from local channel width and
   waterfall drop height.
3. Refine the basin only when its current triangles are too coarse.
4. Carve a smooth bowl using metric radial distance and a smoothstep profile.
5. Keep the deepest point beneath the curtain's impact area rather than at a
   single mesh vertex.
6. Raise gradually toward the lower river surface around the pool perimeter.
7. Carve a definite downstream outlet at the normal downstream channel floor so
   the pool cannot become an isolated sink.
8. Mark the basin as riverbed, remove loose soil or grass coverage, and account
   for removed material through the existing sediment budget.
9. Extend the lower water surface across the pool at the lower terrace level.

Initial pool dimensions should be conservative and bounded. Exact multipliers
should be established from focused generated examples rather than exposed as a
large set of Unity controls immediately.

### Pool stop/go gate

Proceed with plunge-pool support only if:

- the straight waterfall passes its geometry and visual acceptance criteria;
- the pool remains draining and does not interfere with confluences or mouths;
- focused tests cover both eligible and rejected pools; and
- the additional generation time remains acceptably local.

Otherwise retain the broad lower landing and defer the pool without reverting
the waterfall work.

## Phase 8: Unity and collider integration

- Generate the waterfall and any pool before LOD slicing and terrain-collider
  heightmap sampling so render and collision surfaces share the same terrain.
- Keep the vertical curtain render-only; do not attempt to reproduce it in the
  TerrainCollider.
- Ensure the steep supporting terrain cannot leave a gap through which the
  player can fall.
- Continue using the existing river material for the curtain initially.
- Defer foam, spray, particles, sound, and waterfall-specific materials until
  geometry is stable.
- Consider exposing a small set of clearly labelled waterfall and pool settings
  on the Island component only after useful ranges are established.

## Tests

### Focused Rust tests

- a waterfall patch produces distinct full-width upper and lower rings;
- waterfall terrain refinement respects the configured maximum edge length;
- upper and lower constrained heights survive smoothing exactly;
- the supporting terrain remains a valid height field with positive projected
  triangle areas;
- the final channel-clearance pass does not collapse the landing back to one
  vertex;
- the visible curtain contains distinct vertices at matching XY positions and
  different heights;
- curtain triangles have consistent winding, finite normals, and valid UVs;
- ordinary river surfaces stop and resume at the correct waterfall edges;
- an eligible plunge pool forms a smooth basin with a downstream outlet;
- an ineligible pool cleanly falls back to the broad landing;
- pool carving updates riverbed coverage and sediment accounting;
- waterfalls near sea level still respect hard river-mesh clipping.

### Short integration checks

- generate a fixed short seed with at least one waterfall;
- report waterfall lip width, landing width, maximum patch edge, and pool
  eligibility diagnostics;
- verify terrain sea-plane clearance and valid triangle indices;
- verify LOD slices and collider heightmaps remain finite and aligned;
- inspect the result with the black mesh overlay enabled; and
- build and replace the release Unity plugin after Rust checks pass.

### Required validation

- `cargo fmt --all -- --check`;
- focused mesh and river unit suites;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- `git diff --check` for touched files;
- one short complete-island generation smoke test; and
- `cargo build --release --lib` with matching source/plugin checksums.

Long-running generation suites remain optional unless a focused or short smoke
test reveals a broader topology regression.

## Acceptance criteria

The straight waterfall phase is complete when:

- the upper river ends at a clearly defined full-width lip;
- the lower river begins at a full-width landing rather than one centre point;
- the visible curtain can form a straight vertical drop;
- no large triangular fan spans the waterfall or its banks;
- constrained upper and lower edges are unchanged by smoothing;
- the supporting terrain contains no inverted, degenerate, or overhanging
  height-field triangles;
- the player collider cannot pass through or fall behind the waterfall;
- ordinary rivers, confluences, mouths, and sea clipping continue to work; and
- focused tests, strict linting, the short generation smoke test, and the
  release build pass.

The optional plunge-pool phase is complete when:

- eligible waterfalls form a broad, smooth basin beneath the curtain;
- the deepest area is distributed across the impact zone rather than one
  vertex;
- every pool has a continuous downstream outlet;
- pool riverbed and material masks exclude grass correctly;
- rejected pools fall back to the standard lower landing without malformed
  geometry; and
- pool generation remains deterministic and locally bounded.

## Non-goals

- true vertical or overhanging terrain geometry;
- a vertical TerrainCollider face;
- fluid simulation;
- waterfall spray, mist, foam, sound, or particle effects;
- erosion simulation driven by the finished waterfall curtain; and
- forcing a plunge pool where topology or collision constraints make it unsafe.

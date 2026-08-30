# Tree-Trunk Ferns Plan

## Goal

Add deterministic fern colonies close around accepted tree trunks using
procedural quad strips and alpha stencils. Ferns should read as low radial
rosettes from every viewing angle, follow the final terrain, share the existing
wind field, remain LOD 0 only, and add no per-clump GameObjects or colliders.

The implementation should reuse the successful reed/rush ownership, export,
streaming, cutout, shadow, ambient-occlusion, reflection, and distance-fade
contracts while giving ferns a substantially different silhouette.

## Visual construction

### Radial fronds rather than crossed upright cards

A fern clump should contain approximately four to seven fronds distributed
around a central crown. Each frond is a tapered ribbon made from five
longitudinal quad segments. The vertices follow a shallow three-dimensional
curve: the frond rises near the crown, arches outward, then relaxes slightly
toward the ground. This gives wind and perspective enough geometry to bend the
plant without constructing individual leaflets.

The frond ribbon is double-sided. Its procedural alpha stencil draws:

- a narrow central rachis;
- alternating left and right leaflets along the frond;
- progressively shorter leaflets near the crown and tapered tip;
- small deterministic changes to leaflet spacing, angle, and width;
- an unused margin around the card so no rectangular edge or flat clipped tip
  can become visible.

Radial fronds naturally cover all camera angles. This is preferable to four
vertical intersecting cards because a fern's characteristic silhouette is a
low spreading crown rather than a vertical tuft.

### Controlled variation

Use deterministic per-clump and per-frond values to vary:

- four-to-seven mature fronds;
- one or two shorter, more upright inner fronds;
- overall frond length and width;
- crown rotation and imperfect angular spacing;
- arch height and droop;
- leaflet count, taper, and fullness;
- two or three species-shape variants;
- colour tint, stiffness, and wind phase.

Variation must remain bounded so every generated card retains a complete
stencilled tip and no frond intersects the trunk at its base.

## Rust generation boundary

### Module and ownership

Add `island-rs/src/ferns.rs`; do not use `mod.rs`. Define:

- `FernOptions` for validated physical placement and shape controls;
- `FernSurface<'a>` containing only borrowed terrain/material/exclusion fields;
- `FernMeshTile`, matching the reed mesh/material/environment stream shape;
- `FernMeshes`, owning the fixed owner-grid tiles and sorted support vertices;
- `generate_ferns`, returning one owned result after forest assembly.

Store one `FernMeshes` value on `Island`. The generator should borrow
`&Terrain`, `&ForestMeshes`, and the relevant final material masks. It should
not clone tree placements, trunk meshes, terrain fields, or adjacency.

Generate ferns after `generate_forest` because the accepted tree anchors and
final LOD2 trunk geometry are authoritative. Generate them before the final
forest-floor/environment and exported terrain-material fields are assembled so
fern support triangles can affect the ground treatment.

### Trunk authority

Zip `ForestMeshes::placements()` with the final LOD2 trunk-collider iterator.
Validate that their lengths and ordering agree. Use:

- `TreePlacement.anchor` as the colony centre and owner seed;
- `TreePlacement.scale` to adjust the outer colony radius modestly;
- `ForestTrunkCollider.radius` as the real inner collision boundary;
- the collider bottom rather than an assumed procedural-tree origin when a
  terrain-fitted trunk has moved vertically.

The minimum fern radius should be `trunk radius + bark clearance`, not a fixed
guess. A conservative initial bark clearance is 0.20 m. A practical default
outer radius is roughly 1.6 m for a scale-one tree, increasing only modestly
with tree scale so adjacent colonies do not fill the whole forest floor.

## Placement algorithm

### Candidate distribution

For each accepted tree:

1. Form a physical annulus from the final trunk radius plus bark clearance to
   the scale-adjusted outer radius.
2. Derive a stable tree-specific rotation and candidate sequence from the
   island seed, terrain vertex, and a dedicated fern seed domain.
3. Generate candidates with a jittered golden-angle or stratified annulus
   sequence. Use square-root radial sampling so area density is uniform and no
   visible rings form.
4. Multiply a coherent understory noise field by sampled loose-soil richness.
   This creates colonies and gaps rather than an identical halo around every
   trunk.
5. Enforce a global physical spacing with a spatial hash so neighbouring tree
   colonies cannot stack clumps on one another.

The number of attempted candidates should derive from annulus area and spacing,
not from a silent island-wide cap. A configurable coverage threshold and
spacing should control density predictably in world metres.

### Final-terrain sampling

Add or expose a crate-private terrain sampling result that returns position,
normal, supporting triangle indices, and barycentric weights in one lookup.
For each candidate:

- sample the final LOD0 surface at the candidate XY;
- reject points outside the mesh or at/below sea level;
- reject slopes over the configured maximum;
- interpolate deposited depth and require useful loose soil;
- reject river-bed, forced-rock, settled-stone, and reed-support triangles;
- reject snowline terrain and visually classified beach terrain;
- reject any point inside the trunk collider plus bark clearance;
- retain the supporting triangle for ground-material integration.

Using one support sample prevents the position, normal, material, and support
triangle from disagreeing after adaptive tessellation.

### Root and ground fit

Place the fern crown directly on the sampled surface, then sink it only a few
centimetres to hide numerical gaps. Build the first frond row in the sampled
tangent plane. The inner frond segment should conform to that plane while the
remaining segments follow the authored arch. This avoids floating card corners
on sloped ground without dragging every frond tip into the terrain.

## Mesh and vertex contract

Use the same 64×64 complete-owner grid as reeds. Assign each whole clump to the
tile containing its crown; never clip frond triangles at tile boundaries.

Recommended per-frond geometry:

- five longitudinal quad segments;
- twelve vertices per frond if rows are shared;
- ten triangles per frond;
- four-to-seven fronds per clump;
- approximately 48–84 vertices and 40–70 triangles per clump.

Keep all parallel streams finite and exactly vertex-sized:

- position: curved ribbon in normalized island coordinates;
- normal: ribbon face normal suitable for `VFACE` correction;
- UV0: across-frond and crown-to-tip coordinates;
- UV1/environment: normalized clump crown XY for shared wind sampling;
- colour/material RGBA: species variant, tint, flexibility, and phase.

Bake geometric arch, azimuth, and length into positions. Keep only values that
must vary in the shader in the per-vertex data; do not add a new export format
for data that can be represented by the established mesh streams.

## Terrain integration

`FernMeshes` should retain a sorted unique list of every terrain vertex from
its accepted support triangles. Before exporting terrain fields:

1. union fern support into the forest-floor mask so the authored forest-floor
   material extends under each colony;
2. apply the existing loose-cover grass-suppression cap to fern support
   vertices so fur grass cannot protrude through the frond cards;
3. leave hardness, river, sea-proximity, stone, and forest-placement inputs
   unchanged.

This is a final render-material adjustment only. It must not feed back into
erosion, tree selection, trunk scaling, or fern eligibility.

## Native API and compatibility

Add a validated C-compatible `MotuFernOptions` with explicit scalar fields and
compile-time size assertions. Keep existing constructors working unchanged.
Introduce a new most-complete constructor, for example
`CreateMotuWithForestReedsAndFerns`, while older constructors forward default
fern options internally.

Add `CreateFernMeshGrid` and reuse `ExportMeshGrid`/`ReleaseMeshGrid`. Validate
that the grid contains exactly 4096 owner tiles and that every non-empty tile
has matching position, normal, UV, material, and environment lengths.

The C header, Rust FFI types, C# interop structs, ABI validation, null-handle
behaviour, and allocation/release lifecycle tests must change together.

## Unity preparation and streaming

Copy fern tiles on the existing background generation worker into
`IslandPreparedMesh[]`. Validate all indices and parallel attributes before the
native buffers are released.

Extract the common parts of `ReedTileStreamer` into a small reusable LOD0
cutout-vegetation owner-grid streamer, or add a matching `FernTileStreamer` if
the extraction makes the first implementation harder to review. In either
case:

- fern renderers use the same active 3×3 LOD0 neighbourhood as terrain;
- there are at most nine active fern renderers;
- no GameObject is created per tree, colony, or frond;
- leaving first-person focus releases the uploaded fern meshes;
- visibility toggles do not regenerate the island;
- regeneration disposes all old meshes and roots.

Add serializable fern settings for visibility, annulus clearance/radius,
spacing, coherent patch size and threshold, frond-count range, frond-size
range, maximum slope, two display colours, and wind strength. Keep colours and
wind on the Unity rendering side; pass only generation-relevant values to Rust.

## Fern shader

Create `Motu/Forest Ferns` as a dedicated double-sided alpha-test shader.

### Silhouette

The fragment function should generate the rachis and alternating pinnate
leaflets from UV0 and variant data. Use `fwidth`-based antialiasing,
`AlphaToMask On`, and an alpha clip. Explicitly taper the final leaflets and
leave an unused card margin so neither side nor tip can expose a rectangle.

### Lighting and wind

Reuse `GrassWindCommon.cginc` and the exact world-space wind inputs used by
grass, trees, and reeds. Pin the crown and apply displacement approximately as
`v²`, modified by stiffness and phase. Because fern fronds are mostly
horizontal, include a small vertical flutter component as well as horizontal
bend, but keep displacement bounded below the frond length.

Flip the normal with `VFACE` for back faces. Blend base/tip colour along the
frond and add subtle deterministic tint variation without changing alpha.

### Auxiliary passes

- Write a custom shadow caster that applies the identical fern stencil and
  distance fade; solid quad shadows are unacceptable.
- Use a dedicated render type such as `MotuFernCutout` so the current realtime
  ambient-occlusion depth-normal replacement excludes the cards, matching the
  reed fix.
- Add a simplified `MotuReflection="Ferns"` replacement subshader with the
  same alpha silhouette and broad wind.
- Use coherent/dithered distance fade before the active LOD0 boundary so whole
  owner tiles never pop visibly.

## Performance budget

Initial guardrails:

- LOD0 only;
- no colliders;
- no per-clump objects or materials;
- maximum nine draw calls in the active neighbourhood;
- four-to-seven fronds and five segments per frond by default;
- conservative outer radius and physical spacing;
- no individual leaflet geometry;
- no sampling or allocations per frame beyond existing tile activation.

Profile alpha overdraw as well as triangle count. If overdraw dominates, reduce
frond width/count before reducing stencil quality or adding an abrupt fade.

## Validation

### Rust

- reject non-finite and invalid option ranges;
- deterministic output for identical seed/options;
- no forest produces no ferns;
- every clump belongs to an accepted tree annulus;
- every root is outside the actual trunk collider plus clearance;
- final sampled height, slope, soil, river, rock, stone, reed, beach, sea, and
  snow exclusions are respected;
- global spacing holds across neighbouring tree colonies;
- support vertices are sorted, unique, and in range;
- whole clumps remain in one owner tile;
- mesh indices and all parallel streams are finite and complete;
- forest-floor union and grass suppression change only intended channels;
- FFI allocations have matching release behaviour.

### Unity/static

- C#/native layouts and option forwarding match;
- copied tile attributes and indices validate before native release;
- shader and reflection replacement subshader compile on Metal;
- main and shadow passes use the same silhouette;
- AO replacement does not draw solid fern ribbons;
- visibility, first-person focus, regeneration, and disposal are safe;
- no more than nine fern renderers are active.

### Live visual and performance QA

Inspect sparse and dense forests, small and large trees, slopes, river-adjacent
trees, stones, forest fringes, shadows, reflections, wind, and LOD transitions.
Look specifically for card rectangles, clipped tips, fronds entering trunks,
floating crowns, fur grass protrusion, synchronized wind, alpha shimmer, and
frame-time or overdraw regressions.

## Delivery sequence

1. Add validated fern options, borrowed surface inputs, deterministic annulus
   placement, and focused Rust tests.
2. Add curved radial frond construction, support tracking, owner tiles, and
   finite/parallel-stream tests.
3. Integrate after forest generation and union fern support into forest-floor
   and grass-suppression fields.
4. Add native export, managed preparation, ABI/lifecycle validation, and the
   complete constructor while retaining old entry points.
5. Add LOD0 streaming and Unity settings/material creation.
6. Add main, shadow, AO-exclusion, and simplified-reflection shader paths.
7. Run formatting, the full Rust suite, strict Clippy, signed release build,
   matching-checksum deployment, Unity batch native validation, and live visual
   tuning.

## Acceptance criteria

- Ferns form irregular radial colonies close to trunks rather than uniform
  rings or upright crossed-card tufts.
- No fern intersects its owning trunk, floats above terrain, or appears below
  sea level.
- Individual quad boundaries are not visible in colour, shadows, ambient
  occlusion, or reflections.
- Ferns sway coherently with the existing vegetation while their crowns remain
  pinned.
- Terrain beneath colonies reads as forest floor/dirt and grows no fur grass.
- Only LOD0 neighbourhood tiles render ferns, with bounded draw calls and clean
  fades.
- Existing forest, reed, terrain-material, native-constructor, and streaming
  behaviour remains backward compatible.

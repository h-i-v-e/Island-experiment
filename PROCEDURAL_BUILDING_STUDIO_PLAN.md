# Procedural Building Studio plan

Status: initial single-storey implementation delivered, 10 November 2026.
[`island-building`](island-building/README.md) now contains the core generator,
three LODs, standalone studio, recipes, and portable exporter. `planet-bevy`
contains terrain placement, controller collision, and scene persistence.

The architecture and acceptance plan below remains the design record. Current
implementation constraints are rectangular single-storey shells, parallel room
partitions with open internal passages, fixed transparent windows, and gable or
single-slope roofs. Each generated building has at least one hinged exterior
door. Multi-storey grammar and Unity integration remain later milestones.

Verified locally: 30 new-crate library tests, 13 core integration tests, all 194
Planet tests, strict new-crate Clippy, and strict Planet production-binary Clippy.
Exterior LODs, the empty interior, transparent windows, and closed/open doors have
been inspected in actual rendered captures. A 64-instance headless ECS check
verified shared assets, automatic LOD changes, independent doors, and cleanup;
its timing figures measure CPU updates and exclude GPU rendering.
The signed macOS app also passed a live walkthrough: closed-door blocking,
keyboard opening, entry, occupied-interior LOD0 pinning, and retained open-door
state through exterior LOD2 and back. Native Planet review also verified H placement,
closed/open door entry, and scene save/load with retained open-door state. See [validation notes](island-building/VALIDATION.md)
for commands, evidence boundaries, and review artifacts.

Door material recipes, separate metal knobs, and the shared wooden plank recipe
are also implemented and verified; see the validation notes for the follow-up.

## Outcome

Add a standalone Building Studio, following the Tree Studio's visual workflow
and Material Studio's saved JSON recipe workflow. A recipe and seed produce a
repeatable building with an unfurnished, accessible interior, at least one
openable exterior door, and windows that can be seen through at close range.

Every complete building has exactly these three visual representations:

| Level | Interior | Windows | Exterior | Doors |
| --- | --- | --- | --- | --- |
| LOD0 | Walkable floors, wall thickness, ceilings, roof underside, and any room partitions | Real openings with transparent glass; visible from inside and outside | Full authored geometry and material detail | Separate hinged leaves with interaction and collision |
| LOD1 | No interior render geometry | Opaque panes closing the window openings | Simplified shell, roof, large frames, and important silhouette features | Simple distance proxies; instance state is retained |
| LOD2 | No interior render geometry | Baked opaque window appearance | Minimal three-dimensional shell with a baked exterior texture atlas | Appearance baked in a canonical closed pose; instance state is retained |

LOD2 remains a three-dimensional building, so it works from different directions
and elevations. All levels share the footprint, floor elevation, roof shape,
opening positions, orientation, and material palette of one resolved design.

## Existing patterns to follow

The current source, inspected for this plan, provides these starting points:

- [`island-tree`](island-tree/README.md) owns deterministic prototypes, a Bevy
  compiler, an interactive studio, three review LODs, and repeatable captures.
- [`island-material-studio`](island-material-studio/README.md) owns typed JSON
  documents, undo/redo, protected atomic saves, background previews, stale-result
  rejection, and explicit baking.
- [`island-rs::procedural_textures`](island-rs/src/procedural_textures.rs) owns the
  shared material evaluator, physical texture scale, typed maps, and output
  writer. Buildings should consume this evaluator for their material recipes.
- [`planet-bevy/src/collision.rs`](planet-bevy/src/collision.rs) currently provides
  terrain, rock, and log queries. Building walls, floors, and moving doors need
  an explicit addition to the exploration controller's collision contract.
- Unity's [`TerrainFoundation`](packages/com.motu.runtime/Runtime/World/TerrainFoundation.cs)
  already fits a level stone platform to a terrain footprint. Its current bounds
  measurement needs care around moving doors and changing active LOD children.
- The root Cargo workspace contains the Planet crates. The Island studios use
  independent workspaces and manifest-based commands.

Use these as patterns and existing APIs. Do not make the building generator
depend on an editor application's document types or introduce another material
evaluator.

## Initial scope

Make the first complete release support single-storey, rectangular buildings:

- A small hut, a cottage with room partitions, and a larger shed/workshop.
- Gable and single-slope roofs, configurable eaves, wall thickness, floor and
  ceiling heights, and optional simple entrance steps.
- Configurable exterior doors and windows, with repeated facade patterns and
  explicit overrides for individual openings.
- Empty rooms with finished structural surfaces. No furniture is generated.
- Timber, plaster, masonry, and roof material slots backed by existing or newly
  authored Material Studio recipes.
- All three LODs, door interaction, collision, save/reopen, and export.

Start with the hut as the end-to-end proof. Add the other presets only after its
interior, glass, doors, LOD switching, and bake are working. Multi-storey layouts
with connected stairs, joined wings, balconies, curved walls, destructibility,
furniture, and automatic settlements are later extensions. Windows need not
open in the first release.

The proposed delivery target is the standalone Bevy studio first, then
`planet-bevy` as the first world consumer because it is the repository's current
default viewer. The Island viewer and Unity remain additional adapters over the
same generated data. This is a planning assumption, not a restriction on the core.

## Ownership and module boundaries

Add an independent sibling crate, `island-building`, following the Tree Studio's
prototype boundary while allowing a renderer-free build:

```text
island-building/
  Cargo.toml
  README.md
  recipes/                 committed example building recipes
  src/
    lib.rs                 engine-neutral public API
    recipe.rs              typed, versioned authoring model
    validation.rs          dimensions, topology, access, resource limits
    layout.rs              resolved rooms, walls, roofs, and openings
    geometry.rs            LOD0 structural surfaces
    lod.rs                 LOD1 and LOD2 derivation
    collision.rs           static shapes and moving-door descriptors
    materials.rs           references to shared texture recipes
    bake.rs                exterior atlas generation
    export.rs              versioned portable output and manifest
    render.rs              optional Bevy asset compiler
    interaction.rs         optional Bevy door and LOD systems
    document.rs            studio document and history
    studio.rs              studio controls and preview scheduling
    main.rs                building-studio binary
```

Use file-named module roots, with subdirectories as these modules grow. Keep
functions focused rather than letting geometry or UI become one large module.

- The default library exposes recipes, generation, validation, collision data,
  and CPU baking without Bevy dependencies. Reuse `island-rs` without its default
  GPU-generation feature for material evaluation and suitable mesh types.
- A `bevy` feature adds mesh/material compilation and runtime components.
- A `studio` feature adds the Bevy application, egui, documents, and file dialogs;
  the `building-studio` binary requires this feature.
- Keep the crate outside the root workspace, matching the other Island tools.
  Runtime consumers enable only the features they need.
- Building-local geometry uses metres and native Z-up. Adapters explicitly
  convert to renderer axes and world units. Planet placement supplies a local
  tangent frame with radial up; the generator knows nothing about planet radius.

The generation pipeline is:

```text
BuildingRecipe + seed + resolved material dependencies
  -> validated BuildingLayout with stable element IDs
  -> LOD0 meshes + LOD1 meshes + LOD2 shell
  -> collision shapes + door hinges + room/portal metadata
  -> explicit exterior bake -> atlas + complete prototype manifest
  -> renderer compilation -> shared assets + individual building instances
```

Generate the layout once. Borrow it for geometry, collision, validation, and
baking. Compiled prototypes own immutable meshes and textures; each placed
instance owns its transform and door state. Many matching instances can share
the same prototype assets.

## Recipe contract

The typed JSON recipe should describe:

| Group | Contents |
| --- | --- |
| Identity | Schema version, name, seed, archetype |
| Structure | Width, depth, floor elevation, wall/ceiling heights, thicknesses |
| Roof | Type, pitch, eaves, thickness, material slot |
| Layout | Room partitions and internal doorways with stable IDs |
| Facades | Window/door rules, spacing, and explicit opening overrides |
| Doors | Width, height, leaf thickness, hinge side, swing direction, angle limit |
| Windows | Dimensions, sill height, frame profile, pane divisions, glass tint |
| Materials | Recipe references, physical tiling, supported colour parameters |
| Variation | Bounded variation in proportions, placement, and weathering |
| Output | LOD simplification and atlas size/quality settings |

Resolve referenced texture recipes relative to the building recipe, and include
their content hashes and parameter values in the build identity. Export a
self-contained dependency snapshot so a moved building package remains usable.
Report missing materials explicitly.

Use separate deterministic random streams for structure, openings, and surface
variation. Changing wall colour must not move windows or regenerate a different
floor plan. Keep schema, geometry algorithm, material algorithm, and bake
versions distinct so cache invalidation is explainable.

Validation must reject or clearly diagnose:

- Invalid/non-finite dimensions, excessive counts, and output memory budgets.
- Openings that overlap, cross a corner, exceed a wall, or leave insufficient
  structural space. Eaves must not intersect window or door tops.
- Missing exterior doors, insufficient player clearance, and blocked door swings.
- Rooms without a walkable route to an exterior entrance. A window is not an
  access route. Derive room/door connectivity from resolved geometry.
- Floors, thresholds, partitions, or ceilings that obstruct the entrance path.
- Unsupported roof/layout combinations and unknown recipe versions.

Set clearance defaults against the target player dimensions during the first
runtime proof. Use those same dimensions in generation checks and walkthrough
acceptance. Do not silently repair an invalid authored layout into a different
building; show which element needs changing.

## Geometry, glass, and doors

Build walls as solid sections around openings, including jambs, sills, lintels,
and inner faces. Wall triangles and collision must leave actual door/window
holes. Generate floors and ceilings with consistent winding and no coincident
surfaces. Partition connections must not leave cracks or duplicate wall faces.

Keep opaque surfaces, glass, and moving doors separate. Glass begins with a
simple lightly tinted transparent material and modest reflections. The required
result is a clear view into and out of the room under actual lighting. Review
depth, shadowing, transparency sorting, and overlapping windows from both sides
early; advanced refraction is optional.

Export each door's stable ID, local hinge transform, closed pose, angular limits,
interaction bounds, and leaf collider. The runtime opens/closes it with a bounded
kinematic animation. Sweep the leaf against the player and nearby blockers;
pause or reverse when obstructed. A closed door blocks passage, while an open
door leaves the doorway clear. Interaction must work from both sides.

Door angles and targets belong to instance state, independently of visual LOD.
Key saved state by placement ID and door ID so streaming or reloading cannot
reset a door. Define a migration/default policy when an edited recipe removes or
adds doors. Dynamic door state never changes the shared prototype hash.

Collision is generated from structural data, independently of visual meshes:
wall sections around openings, floor/ceiling slabs, fixed glass, and door leaves.
Keep nearby collision resident through visual LOD changes. Distant collision can
unload only beyond the interaction/physics residency range. Never substitute a
solid box while a player can reach a doorway.

## LOD derivation and switching

LOD1 retains the silhouette and large features, removes all interior render
surfaces, simplifies frames and trim, and closes windows with opaque panes.
Those panes use an authored glass/proxy appearance consistent with the building
palette. They must not depend on a now-missing interior for their shading.

LOD2 keeps the simplest shell that preserves the roof and major silhouette
features. Window frames, siding, small trim, and door details become texture
content. A large porch roof or chimney remains geometry when its outline matters.

The runtime and studio support Auto, forced LOD0, forced LOD1, and forced LOD2.
Auto selection uses projected building size and hysteresis, with thresholds
calibrated in the walkthrough scene rather than one distance for every size.

- Pin LOD0 while a player or required viewing camera is inside, near an entrance,
  within window-inspection range, or interacting with a door.
- Include opening proximity in the LOD decision; distance to the building centre
  alone is insufficient for a large building.
- Rebuild the full representation before entering interaction range. Hold the
  current complete representation until its replacement is ready.
- Restore retained door poses before LOD0 becomes visible. Static closed-door
  proxies are acceptable only outside the range where the difference is visible
  or interactive. Never close the actual door state to match a bake.
- Switch the glass/interior pair together. Do not independently alpha-fade glass,
  opaque panes, and disappearing interior walls. If opaque geometry uses a
  dithered transition, review glass separately for double images and depth holes.
- Track occupants independently of the rendering camera. Collision, occupancy,
  and interaction must not disappear when the camera leaves or changes mode.

Starting complexity targets for the small dwelling presets are 10–30k triangles
at LOD0, 1–5k at LOD1, and 50–500 at LOD2, with approximately 1–2 opaque material
draws at LOD2. These are profiling targets, not measured promises. The studio
reports actual triangles, draws, texture memory, build time, and bake time.

## LOD2 exterior baking

Use a deterministic CPU bake over the final simplified mesh, building on the
shared material evaluator. Parameterized wall and roof surfaces allow explicit
charts for the initial rectangular buildings; arbitrary mesh unwrapping is not
required for the first release.

1. Assign non-overlapping charts to every wall, gable, roof face, and retained
   silhouette feature, with appropriate texel density.
2. Sample the detailed exterior onto those charts, using its material UVs and
   physical scale. Project frame/trim/door detail onto the receiving shell.
3. Substitute opaque window surfaces and closed doors for baking. Exclude the
   interior so panes cannot accidentally bake transparent holes.
4. Bake albedo, tangent-space normals, roughness/material masks, and restrained
   local occlusion. Do not bake sun direction, sky, or time-of-day lighting into
   albedo. Define normal and channel conventions in the manifest.
5. Dilate chart borders and generate mip-safe padding. Inspect roof edges,
   window borders, normal seams, and distant mip levels.
6. Write the mesh, maps, dependencies, and manifest transactionally. Include the
   resolved recipe/dependency hashes, generator/baker versions, and settings.

Start with a 1024-pixel atlas for a small dwelling and expose lower/higher
resolutions under a texture-memory limit. Use reduced-quality preview bakes
while editing and an explicit final Bake action. A material or opening edit must
invalidate the relevant atlas; an instance door opening must not trigger a bake.

## Studio workflow

Use the established studio arrangement: a central 3D view, tabbed parameters,
recipe/preset selection, compact telemetry, and camera controls clear of input.

- New, Open, Save, Save As, Revert, Validate, Undo/Redo, and Bake/Export.
- Structure, Layout, Openings, Roof, Materials, and LOD/Bake tabs.
- Orbit, facade, entrance, interior, and collision-enabled walkthrough views.
- Auto/manual LOD, wireframe, collision overlay, room/access diagnostics, and
  an editor-only cutaway for inspecting internal construction.
- Door controls plus direct interaction during walkthrough; light/exposure
  controls that do not change the building recipe.
- Material recipe selection and parameters, with paths usable in Material Studio.
  Keep full material editing in Material Studio initially.
- Seed changes and preset thumbnails, with protection for unsaved changes.

Follow Material Studio's atomic saves, external-change handling, bounded history,
coalesced slider transactions, and background preview scheduling. An owned recipe
snapshot crosses the worker boundary; keep one running and one newest pending
job. Reject stale results and replace complete mesh/material sets together.

Invalid drafts keep the last good preview and display element-level errors.
An expensive atlas bake must not block geometry editing. Report when the atlas
belongs to an older revision. A geometry replacement during walkthrough must
move the viewer to a safe entrance/inspection position or wait until walkthrough
ends; it must not enclose the player in newly generated walls.

## Runtime and terrain integration

Keep placement outside the building generator, as with the tree prototype API.
The initial studio uses a flat inspection ground; the first world integration
adds an explicitly placed building rather than procedural town generation.

For Bevy consumers, compile shared meshes/materials once per prototype. Instances
have one root plus the small set of dynamic doors and necessary render parts;
avoid an entity for every wall board, pane division, or trim piece. Add building
shape queries to the actual exploration collision path and interaction raycasts.
Integrate `planet-bevy` first under the proposed delivery scope; neither the
Planet controller nor the Island viewer should be described as supporting
buildings until tested.

For Planet, transform the local building and foundation to a tangent frame at
the placement anchor, preserving metre dimensions through the viewer's normalized
scale. Test door hinges and player clearance away from the poles and equator.

For Unity, keep reusable runtime components in `packages/com.motu.runtime`, with
demo setup in `island-unity`. A later adapter can consume the same exported mesh,
material, collision, and door metadata. Validate the export boundary before
adding a new native ABI merely for buildings. Unity acceptance is a separate
milestone from the first Bevy implementation.

Terrain placement must query the complete static footprint, reject unsuitable
slope/water conditions, and produce a level floor with usable entrance access.
Reuse the foundation's terrain-query/fitting policy where applicable. Use a stable
footprint from layout metadata rather than bounds that change with door angle or
active LOD. Check the doorway approach and steps against terrain, vegetation, and
rocks; an unobstructed internal room graph alone does not prove access.

## Implementation sequence and acceptance

Each stage ends with a reviewable result. Complete the three LODs before expanding
the building grammar into multi-storey or settlement generation.

| Stage | Deliverable | Acceptance gate |
| --- | --- | --- |
| 1. Contracts and test hut | Typed JSON, validation, stable layout IDs, floor/walls/roof, one door and two windows | Same recipe produces the same layout; real openings and player dimensions are established |
| 2. Walkable LOD0 proof | Lit preview, transparent glass, door interaction, structural collision | Enter and leave from either side; see through windows; closed door blocks; open door clears; no missing wall/ceiling faces |
| 3. Three representations | LOD1 shell/proxy windows, LOD2 shell and exterior atlas, Auto/manual LOD | No interiors or glass draws in LOD1/2; inspect every facade/roof and transitions; occupancy and door state survive switching |
| 4. Recipe studio | Full document lifecycle, controls, history, previews, diagnostics, export | Save/reopen reproduces output; stale jobs cannot replace current previews; invalid edits preserve last good view |
| 5. Initial recipe set | Hut, partitioned cottage, shed/workshop; both roof families and material variation | Every room is reachable, entrances fit, recipes validate, all LODs bake and render correctly |
| 6. Planet world integration | Placement, foundation, terrain approach, collision/residency, retained instance state in `planet-bevy` | Walk from real terrain through the door; leave/revisit and unload/reload; exercise several placed buildings |
| 7. Additional adapters and grammar | Unity adapter and/or additional Bevy consumer; later multi-storey/wings | Separate runtime acceptance; upper floors require working stairs, collision, and connected access |

Keep early proof controls small. Stage 2 proves the difficult runtime behaviour;
stage 4 supplies the complete authoring workflow after that foundation works.

## Verification

Automated checks should cover meaningful contracts:

- Recipe round trips, version rejection, deterministic output, dependency hash
  changes, stable IDs, and independent random streams.
- A bounded seed/parameter corpus covering small and large valid dimensions,
  corner openings, conflicting partitions, and invalid entrance clearances.
- Wall openings, valid indices, finite/non-degenerate geometry, surface winding,
  collider clearance, roof enclosure, and room/exterior connectivity.
- Hinge transforms, blocker sweeps, retained door state, LOD hysteresis, and
  occupied-building residency. Include an observer moving away from an occupant.
- Atlas coverage, deterministic pixels, material/normal conventions, chart
  padding, stale-bake invalidation, and interrupted export cleanup.
- Save conflicts, undo transactions, and stale background-preview rejection.

Add repeatable captures following Tree Studio: fixed recipe/seed, exterior views
from all sides, an interior view, open/closed doors, and each forced LOD. Capture
transition bands and review bright/daylight and low-light interiors. Visually
inspect exported maps and final rendered PNGs, including distant mip levels.

Run the new crate's focused tests, formatting, and Clippy, then check only the
affected consumers. Measure one detailed building and a group of repeated
instances for generation stalls, draw count, memory, and streaming cleanup.
Record structural tests, image inspection, and live walkthrough acceptance
separately; passing compilation does not establish that glass or doors work.

The first release is complete when an authored recipe can be saved, reopened,
generated, walked through, exported, placed in the first chosen world consumer,
and viewed at all three requested LODs with working door and residency behaviour.

# Island Building Studio

An independent procedural-building crate and Bevy studio for single-storey huts,
partitioned cottages and workshops. Recipes describe metres in native right-handed
Z-up coordinates. The Bevy adapter converts `(x, y, z)` to `(x, z, -y)`.

Run from the repository root:

```sh
cargo run --manifest-path island-building/Cargo.toml --features studio --bin building-studio
cargo run --manifest-path island-building/Cargo.toml --features studio --bin building-studio -- --recipe island-building/recipes/cottage.json
```

On macOS, package an ad-hoc signed app with the debug executable already built in
the shared repository target:

```sh
island-building/package-macos.sh --debug
open "island-building/dist/Building Studio.app"
```

The default package command builds release; `--debug` uses incremental development
artifacts. `--no-build` reuses an executable you have already verified is current.
The bundle includes the three building recipes, the wooden plank, coursed stone, slate and thatch material recipes,
and this README in `Contents/Resources`.
Packaging protects an existing bundle and honours `CARGO_TARGET_DIR`.

The floating inspection controls and left recipe inspector follow Tree Studio's
dark navy/cyan HUD. Right-drag orbits, the wheel zooms, and the named views frame
each facade, the entrance, the interior or the fireplace. Walkthrough uses WASD, Shift to move
faster, right-drag to look, E to interact with a nearby door, and Escape to return
to inspection. **Inspection → Window** frames the first window at an angle for
checking its casing and sill. Closed doors and structural wall sections block
movement; open doorways remain clear. The player capsule is 0.28 m radius and
1.8 m high.

The Structure, Layout, Openings, Roof, Chimney, Materials and LOD/Bake tabs edit the typed
recipe. Layout supports parallel full cross-walls with internal doorways;
crossing partitions are diagnosed. Material selectors accept the same JSON
recipes as Material Studio, resolved relative to the building recipe. Full
material editing remains in Material Studio. Wall, roof, door and chimney materials each
accept an independent recipe. **Materials → Use wooden planks on doors** assigns
the bundled vertical-board recipe; **Door material → Choose JSON…** accepts any
shared material recipe. Clearing the reference restores the door palette colour.
The reusable source recipe is [`wooden-planks.json`](../island-rs/texture-recipes/wooden-planks.json):
six boards across a 1.2 × 2.4 m tile with editable grain, bevelled seams, colour,
and optional staggered end joints.

LOD0 windows have full-depth painted casings, recessed sashes and glazing bars
fitted around the clear glass. Sloped sills project 12 cm outside the wall and
continue into the room. Casing and sills use the trim colour. LOD1 keeps the
previous simplified window frame and opaque pane; LOD2 keeps its baked exterior.

The **Chimney** tab enables one wall-attached fireplace and roof-penetrating
chimney, with editable facade, offset, firebox, hearth and stack dimensions.
New presets include one; existing recipes without a chimney keep their previous
shape. LOD0 includes a recessed firebox, soot-darkened lining, stone breast,
hearth, straight-sided breast to the ceiling and hollow flue.
A projecting mantel and raised surround share the adjacent interior walls' material
and colour. The flue narrows inside the breast, out of sight. The hearth blocks
walking. LOD1 keeps a
simplified exterior chimney, and LOD2 uses an atlas-textured stack. Roof and
ceiling openings follow the flue. Fire and smoke are not generated.

The **Roof** tab offers **Metal**, **Slate** and **Thatch** coverings independently
of the gable or single-slope shape. Selecting a covering restores its default
material and colour; the Materials tab can override it with a custom JSON recipe.
Slate uses staggered courses and shallow overlapping lips. Thatch uses straight,
thick roof runs with horizontal eave caps and a separate solid ridge prism.
The coat is at least 0.45 m thick. LOD0 applies one Catmull-Clark subdivision and
smooths the roof runs and ridge cap separately, then fits each piece back to its
original dimensions. Short supporting edges retain the broad slopes and distinct
ridge shoulder. Straw tips, cut-end shells and hazel bindings follow the smoothed
surface. LOD1 retains the original prism profile with simplified surface detail;
LOD2 bakes the material onto the shell. Both coverings retain the chimney opening.
Recipes without a covering field keep the original standing-seam metal roof.
The thatch ridge reaches about 1.1 m down each slope, with a distinct shoulder
above the main coat; its width follows physical slope length on steep roofs.
It forms one filled triangular prism across the gable apex, with a flat underside
and closed ends projecting 12 cm beyond the main roof at both gables. Packed reed
end grain covers the front faces, shoulder edges and exposed undersides, using
the assigned roof material's colour.
At LOD0, exposed straw ends use
four thin masked shell layers, reaching 20 mm from the cut faces. Nested reed
masks become finer and sparser toward the tips and inherit the roof's material
colour. **Inspection → Roof end** and **Roof under** frame this detail for close
review; **Eaves** shows the ridge and horizontal lower caps from standing height.
LOD1 and LOD2 retain the solid prisms while omitting the straw shells.

The reusable [`slate-roof.json`](../island-rs/texture-recipes/slate-roof.json) and
[`thatch-roof.json`](../island-rs/texture-recipes/thatch-roof.json) recipes are also
editable in Material Studio. Their `roof_colour` parameter recolours pigment
without changing relief. Texture U follows the slope and V follows the ridge.

The chimney and fireplace share an assignable material. With no JSON reference,
they use the embedded [`coursed-stone.json`](../island-rs/texture-recipes/coursed-stone.json)
recipe: staggered stone courses with recessed mortar over a 2 × 2 m tile.
The chimney palette swatch recolours the stone; a custom material recipe supplies
its own colours. Choose any shared masonry or brick recipe with **Choose JSON…**;
**Clear** restores the built-in coursed stone. The soot lining keeps its separate
dark material. **Inspection → Fireplace** frames the interior opening.

New/Open/Revert protect unsaved changes. Save writes a sibling temporary, fsyncs
it, checks the source bytes and atomically replaces the recipe. An external edit
produces a conflict; use Revert to reload it or Save As with a new filename to
preserve both versions. Save As protects existing files. Undo/redo retain up to
96 snapshots and coalesce pointer gestures. Invalid drafts retain the last good
3D preview. Geometry generation runs in the background with one running job and
one newest pending snapshot; stale completions cannot replace the current view.
A successful geometry replacement safely reframes the viewer.

LOD0 includes interior structural surfaces, transparent glass and separate hinged
door leaves with a round metal knob on both sides. Knobs inherit the leaf
transform and disappear with it outside LOD0. LOD1 omits the interior and uses opaque windows. LOD2 is a 3D shell
with a CPU-baked exterior atlas. Auto LOD uses projected size and hysteresis;
openings, nearby interaction and explicit occupants pin LOD0. Door angles belong
to placement state and survive visual LOD changes. Inspection reports triangles,
material draws, texture memory and build/bake time. Collision overlay, wireframe,
roof/ceiling cutaway and lighting controls affect the preview independently of
the recipe.

Sun, sky fill, room light and exposure can also be fixed for review captures with
`--sun`, `--ambient`, `--room-light` and `--exposure`. For example, append
`--sun 0 --ambient 20 --room-light 1200 --exposure 8` for a low-light room.

Preview atlases are limited to 128 px. Bake/Export evaluates the authored atlas
size in a separate worker so editing can continue. The editor reports when a
completed export belongs to an older revision. Committed recipes request 1024 px;
the small built-in presets start at 512 px.

Portable exports require a new destination directory:

```sh
cargo run --manifest-path island-building/Cargo.toml --features studio --bin building-studio -- --recipe island-building/recipes/hut.json --export /tmp/coastal-hut-package
```

A package includes `recipe.json`, `prototype.json`, `lod0.obj`/`lod1.obj`/`lod2.obj`,
`building.mtl`, hinge-local door OBJ meshes, `doors.json`, collision/layout data,
PNG maps, material dependency snapshots and a versioned SHA-256 manifest. Recipe
paths are rewritten relative to the package. Dependencies are verified against
the generated material hashes before publication. Door descriptor order maps
`door-N.obj` and `door-N-hardware.obj` to `doors.json[N]`; each descriptor retains stable ID, hinge,
canonical angle, limits and dimensions. Instance door state stays independent
of the canonical closed-pose atlas. New door IDs default closed; removed IDs are
ignored when restoring placement state.

Thatch exports include the four masked shell materials and separate opacity
maps. The manifest's `masked_materials` entries specify a 0.5 alpha cutoff and
double-sided rendering; OBJ/MTL also references each opacity map. Exported UVs
include each material's physical tile scale.

Albedo maps are sRGB. Normal maps are linear OpenGL tangent-space +Y. Packed
linear masks use R=occlusion, G=roughness, B=metallic, A=255. Export OBJ UVs use
upward V. The manifest lists file hashes and generator/material/bake versions.
Export writes into a sibling staging directory and publishes after every file
has completed; failures remove staging and do not publish an incomplete package.

Repeatable offscreen screenshots work without an interactive window:

```sh
cargo run --manifest-path island-building/Cargo.toml --features studio --bin building-studio -- --recipe island-building/recipes/hut.json --screenshot /tmp/hut-lod0.png --lod 0 --view exterior
cargo run --manifest-path island-building/Cargo.toml --features studio --bin building-studio -- --preset cottage --screenshot /tmp/cottage-lod2.png --lod 2 --view north
cargo run --manifest-path island-building/Cargo.toml --features studio --bin building-studio -- --preset hut --screenshot /tmp/hut-inside.png --lod 0 --view interior --door-open
```

Screenshot filenames must not already exist. `--view` also accepts `south`,
`east`, `west`, `entrance`, `fireplace`, `roof-end`, `roof-under`, `eaves` and `window`; `--capture-ui` includes the HUD. Captures wait for
valid geometry and rendering to settle, then exit after the PNG is written.
Interior/entrance inspection can intentionally pin LOD0 even when a farther LOD
is requested. Use exterior facade views for forced LOD comparisons.

The default library has no Bevy dependency. Consumers use `generate` or
`generate_with_base`, then enable `bevy` for `compile_building`, `spawn_building`
and `BuildingRuntimePlugin`. Compiled assets are shared through `Arc`; each
placement has one scene root and its own retained doors. Attach an explicit
`BuildingObserver` at feet position and, for a detached camera, retain a
`BuildingOccupant` for the player. Runtime consumers own terrain placement and
movement; collision queries do not depend on active visual LOD.

Focused checks:

VS Code uses a pinned Cargo schema in `.vscode/settings.json` to avoid Even
Better TOML's false lint-table errors while retaining manifest validation.

```sh
cargo test --manifest-path island-building/Cargo.toml --no-default-features --lib
cargo test --manifest-path island-building/Cargo.toml --features bevy --lib
cargo check --manifest-path island-building/Cargo.toml --features studio --all-targets
cargo fmt --manifest-path island-building/Cargo.toml --all -- --check
```

Document tests cover coalesced/bounded history, external-change conflicts,
protected destinations and failed-revert preservation. Export tests verify file
hashes, recipe regeneration, retained doors and failed-write cleanup. Rendering
captures and a live collision-enabled walkthrough are separate acceptance checks;
compilation alone does not establish visual or interactive quality. The initial
world adapter is Planet Bevy. Unity import is a separate milestone.

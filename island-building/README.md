# Island Building Studio

An independent procedural-building crate and Bevy studio for single- and multi-storey
huts, partitioned cottages and workshops. Recipes describe metres in native right-handed
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
The bundle includes the building recipes, the wooden plank, floorboard, rough stone, solid slab, daub, slate and thatch material recipes,
and this README in `Contents/Resources`.
Packaging protects an existing bundle and honours `CARGO_TARGET_DIR`.

## Mesh cleanup and experimental texture arrays

All building formulas finalize their assembled LODs before baking and hashing.
Finalization welds identical position/normal/UV vertices and removes collapsed
triangles and exact same-facing opaque duplicates within each material part.
Opposing faces, material seams, glass and masked straw multiplicity are retained.
Generator-specific joins remove covered faces between ceiling cells, masonry
stair blocks and church floor/foundation sections. Church interior wall corners
meet at shared miters, and curved ceiling ribs have caps only at their ends.
Ground-floor undersides are covered by the permanent foundation. Roof-ceiling
tops are enclosed by the roof; Studio cutaway removes roof and ceiling together.
Upper-floor undersides, aperture reveals and exposed soffits remain visible.
Church door/window surrounds own their jamb and pointed-head reveal surfaces;
covered masonry caps are omitted, preserving the exposed strip below raised
door frames.
This does not infer arbitrary hidden surfaces from camera views or colliders.

The optional GPU compiler keeps these source meshes for editing and export while
combining compatible opaque sections using texture arrays and a material index
constant across each triangle. Texture banks retain their native resolutions,
physical UV scale and tangent frames. The shader retains each material's surface
settings and exterior-only grub policy. Glass, masked straw and the baked LOD2 atlas use
the existing material path. Other LOD2 surfaces reuse the same array banks. Entrance stairs remain separately hideable; moving
door leaves can combine their timber and metal hardware without losing the hinge.
Compiled handles are shared between placements; no per-placement pixel copies
are made.

**Batching defaults off because measured gains depend on the workload.**
Use **Inspection → Texture array batching (experimental)** or launch with
`--renderer arrays`; `--renderer standard` selects the existing renderer.
The inspection counter reports compiled mesh sections, rather than claiming to
measure GPU submissions across main, depth and shadow passes.

The offscreen A/B benchmark uses the same geometry, camera, resolution and lights
for either compiler. It waits for shader compilation and for GPU completion on
each frame before collecting synchronized frame intervals. Run serial comparisons with new output paths:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo run --manifest-path island-building/Cargo.toml --features studio --example render_benchmark -- --renderer standard --buildings 32 --variants 1 --lod 0 --output /tmp/building-standard.json
CARGO_TARGET_DIR="$PWD/target" cargo run --manifest-path island-building/Cargo.toml --features studio --example render_benchmark -- --renderer arrays --buildings 32 --variants 1 --lod 0 --output /tmp/building-arrays.json
```

Repeat with 1/32/128 buildings, multiple variants, church and thatched recipes,
and LOD1. `--recipe FILE`, `--screenshot NEW.png`, `--normal-prepass true` and
`--ssao true` allow material, normal/depth/motion prepass and SSAO inspection. Reports distinguish frame
intervals, CPU render diagnostics, compiled sections and image payload bytes.
Metal does not expose GPU timestamps through Bevy's render diagnostics; these
metrics do not claim isolated GPU duration, actual GPU draw calls or driver VRAM.
Build 55 M4 Pro tests showed about 4.3% lower median time for the church scene and
5.8% for varied cottages at LOD1; other cases are unchanged or inconsistent.
Texture payload is equal between compilers. Keep Standard as the general default
and select arrays for workloads where repeated measurements show a benefit.

Runtime validation resumed successfully after other graphics applications were
closed. All 223 Studio tests pass, and the eight-recipe/three-LOD audit removes
11,502 triangles and 90,146 vertices. Standard and array material captures cover
interiors, thatch, open doors, visible/hidden stairs, shadows and SSAO. See
`VALIDATION.md` for the benchmark decision and packaged build evidence.

The floating inspection controls and left recipe inspector follow Tree Studio's
dark navy/cyan HUD. Right-drag orbits, the wheel zooms, and the named views frame
each facade, the entrance, the interior or the fireplace. Walkthrough uses WASD, Shift to move
faster, right-drag to look, E to interact with a nearby door, and Escape to return
to inspection. **Inspection → Window** frames the first window at an angle for
checking its casing and sill. Closed doors and structural wall sections block
movement; open doorways remain clear. The player capsule is 0.28 m radius and
1.8 m high.

The Structure, Layout, Openings, Lean-tos / wings, Roof, Chimney, Materials and LOD/Bake tabs edit the typed
recipe. Layout supports parallel full cross-walls with internal doorways;
crossing partitions are diagnosed. Material selectors accept the same JSON
recipes as Material Studio, resolved relative to the building recipe. Full
material editing remains in Material Studio. Wall, frame timber, roof, door and chimney materials each
accept an independent recipe. **Materials → Use wooden planks on doors** assigns
the bundled vertical-board recipe; **Door material → Choose JSON…** accepts any
shared material recipe. Clearing the reference restores the door palette colour.
The reusable source recipe is [`wooden-planks.json`](../island-rs/texture-recipes/wooden-planks.json):
six boards across a 1.2 × 2.4 m tile with editable grain, bevelled seams, colour,
and optional staggered end joints.

Dimension controls default to **Material units**. **Grid surface** selects the
reference material independently for each dimension tab; **Metres** restores free
metric editing. One U/V unit is a board width/length, masonry block/course, slate
course/width, or thatch course/bundle, derived from the selected recipe's physical
tile size and counts. Other materials use a complete texture repeat. The displayed
metre equivalent stays visible. Edited values snap to whole units; **Snap** rounds
an existing value. Opening or changing the grid does not resize the recipe.
**Reload grid** rereads a material changed externally. Missing or invalid material
recipes show an error and retain metric controls.

Choose the floor or ceiling grid on Structure to author the **inside** width/depth
in board units; the outer footprint includes wall thickness. The foundation grid measures the plinth, including its outset. Other structure
grids measure the outside building footprint. Openings are positioned by their left edge from the
facade's left corner, so odd board/block counts keep both sides on the nominal grid.
The roof grid measures the actual eave-to-top slope and gable-to-gable length;
changing the latter changes building depth. Chimney stack height snaps its absolute
top elevation to courses starting at floor level. Small thicknesses, projections,
clearances and burial margins remain in metres. Dimensions are still saved in metres,
with the usual Undo/Redo and existing recipe compatibility.

## Lean-tos and wings

**Lean-tos / wings** adds up to eight enclosed ground-floor extensions to a
rectangular building. A lean-to has a single roof pitch falling away from the
house; a wing has a gabled roof with its ridge running outward. Choose the host
facade, width along that wall, outward depth, centre offset, eaves height and
roof rise. The main building can have multiple storeys; extensions have one.
The Gothic church retains its separate cruciform formula.

Every attachment is a separate room with a hinged internal door. Covered
windows are omitted and the shared junction is rebuilt as a dividing wall.
Doors use the building-wide door width/height and open into the annex. They
start centred, moving along the divider and choosing a clear hinge side when
necessary. Each extension can select a plank or frame-and-panel door. If the
shared dimensions or a clear approach do not fit, Studio diagnoses the conflict.

Wattle-and-daub buildings use the same timber bays, braces and door crosspieces
on both sides of the divider, ordinary interior partitions and stairwell walls.
The interior daub uses the wall recipe with the interior palette colour. Interior
daub, timber and doors remain free of grub even when exterior weathering is on.

Floors remain level. Wing ridges continue level into the main roof; lean-tos
continue their existing pitch. Intersecting roofs are trimmed against their
actual surfaces, with closed exposed eave undersides at every LOD. Duplicate
floor/foundation faces are removed at joins. The extensions share
the building's wall, timber, roof, floor and ceiling materials, including slate
or thatch. Attached thatched lean-tos omit the high-side ridge cap.
Generated exterior windows use the building-wide dimensions; each extension can
disable windows or override pane rows/columns. Keep entrances, entrance stairs
and chimneys clear of the connections; conflicts show a validation error.

Foundations follow the compound footprint and share a burial depth sufficient
for a 30° slope across the complete building. The terrain preview uses the actual
footprint, including off-centre wings. All three LODs include extensions. LOD0
has connected interiors and transparent glazing; LOD1 has opaque windows and
reduced detail; LOD2 packs the facade bakes into one atlas at the selected
resolution. Increase that resolution if a large compound needs more distant
facade detail. **Inspection → Extension** frames the first internal extension
door from the main building. Use **E** to open it and walk into the room.

`recipes/extensions-cottage.json` demonstrates both types. Code can append the
same typed recipe data, without creating separate building instances:

```rust
use island_building::{BuildingRecipe, ExtensionKind, ExtensionRecipe, Facade};

let mut recipe = BuildingRecipe::cottage();
recipe.extensions.push(ExtensionRecipe {
    id: "east-lean-to".into(),
    kind: ExtensionKind::LeanTo,
    facade: Facade::East,
    width: 3.0,
    depth: 2.5,
    wall_height: 2.4,
    roof_rise: 0.35,
    ..Default::default()
});
```

Offsets follow the host facade's existing tangent convention. Dimensions are
saved in metres, with the same material-unit controls and Undo/Redo as the main
structure. Layout exports include extension bounds, interior bounds, connection
bounds, stable internal door IDs, doorway bounds and window centres; foundation placement metadata encloses the whole
compound. Empty corners of an L-shaped footprint are excluded from occupancy.

## Windows and doors

**Openings → Building-wide defaults** sets one window width, height and sill,
and one door width and height. New internal doorways use these door sizes too;
**Layout → Override door dimensions** gives each partition its own dimensions.
New Studio presets use **Generate openings
automatically**: one entrance per selected wall and evenly distributed windows,
with a configurable target spacing and corner clearance. Entrances start centred;
when a fireplace or partition end blocks the centre they use a clear wall span.
Windows avoid entrances, fireplaces and partition ends. Counts and positions are
recomputed whenever footprint dimensions change; sizes remain the shared values.
Geometry, all three LODs, collision, stairs and bakes use this same resolved layout.
The maximum is 15 windows per wall and 64 exterior openings overall.

Expand a generated opening to move it, **Override dimensions**, change door style,
hinge or swing, or **Suppress this opening**. **Reset to generated values** restores
inheritance. Turning generation off keeps the current resolved openings for manual
editing. Existing saved recipes retain their explicit layout and sizes; **Use
building-wide sizes for all openings** opts a manual layout into shared dimensions,
and each opening can still override them. These edits support Save and Undo/Redo.

Rectangular windows also have **Window panes** columns and rows, each from 1 to 32.
**Single pane** selects a modern 1 × 1 opening; **Shop grid** selects 8 × 4.
Every window follows the building default until **Override window panes** is
enabled on that opening. This is independent of its size override, so a wide
display window can have many small panes while the other windows stay simple.
LOD0 uses real recessed glazing bars, LOD1 uses flat bars over opaque glazing,
and LOD2 bakes the same layout. Gothic church tracery retains its existing pattern.
Open [`shopfront.json`](recipes/shopfront.json) for a 5.2 m wide, 32-pane front
window beside the entrance. Existing recipes keep a 2 × 2 pane layout by default.

For procedural code, no authored opening list is required:

```rust
use island_building::{BuildingRecipe, DoorStyle, OpeningOverride, WindowPanes, generate};

let mut recipe = BuildingRecipe::procedural(9.0, 7.0);
recipe.opening_defaults.window.width = 1.5;
recipe.opening_defaults.window_panes = WindowPanes { columns: 1, rows: 1 };
recipe.opening_defaults.door.height = 2.25;
recipe.opening_generation.as_mut().unwrap().overrides.push(OpeningOverride {
    id: "auto-door-south".into(),
    door_style: Some(DoorStyle::FrameAndPanel),
    offset: Some(0.7),
    ..Default::default()
});
let prototype = generate(&recipe).expect("valid building recipe");
```

JSON uses `opening_defaults` and optional `opening_generation`; `door_facades` and
`window_facades` contain south/east/north/west. Omitting generation preserves the
manual `openings` list, where `use_default_dimensions: true` opts into shared sizes.
In automatic mode that list is derived; edit `opening_generation.overrides` instead.
Set `"window_panes": { "columns": 8, "rows": 4 }` on a manual window or generated
override for a custom pane layout; omit it to follow `opening_defaults.window_panes`.
IDs are `auto-door-south` and `auto-window-east-0`, etc. Window indices count from
the facade's left side, starting at zero. Only specified override fields are fixed;
the rest continue to follow generation. Overrides for absent slots are retained
across resizes and become active if those IDs return. Footprint changes can change
which physical window an ordinal ID addresses. Invalid sizes, overlapping overrides
or removal of every exterior door produce validation diagnostics.

Walls start their U chart at each facade's left corner, and their V courses at floor
level, continuously across opening cuts and gables. Floor/ceiling boards start across
U at the inside wall edge and retain their joist-aligned lengthwise joints. Their grid
uses the authored board length; timber buildings still fit end joins to two joist bays.
Masonry charts use component edges and a common floor-height course phase, including
buried foundations. The same phase is used in distant bakes and portable exports.
Slate relief follows the assigned material's course spacing. Different material grids
can have incompatible module sizes: select the surface whose edges you are aligning.
Running bonds, staggered ends and intentionally irregular stone edges retain their pattern.

**Structure → Storeys** adds up to six occupied storeys, including the ground floor.
**Storey wall height** is the height of each room; floor N is at
`N * (wall_height + floor_thickness)`, with the ground floor at zero. The roof sits
on the top storey. Existing recipes default to one storey. Automatic openings repeat
windows on every floor while exterior entrances remain at ground level. Manual
openings and partitions have a **Storey (0 = ground)** selector. Upper automatic
window IDs append `-storey-N`, allowing the same individual override controls.

Every multi-storey building includes internal **timber U-stairs** with open risers,
two flights between floors, a turn landing, a landing at every occupied floor and
guardrails. Each flight has two continuous timber stringers housing the tread ends;
railing posts sit on the stringers. There are no vertical boards between treads. The floor slabs and exposed joists have real staircase cutouts; floor
landings meet the boards without overlapping faces. Placement searches for a clear
position and orientation, considering doors, partitions and the fireplace/flue.
**Internal staircase** controls its width, tread going, maximum riser, landing depth,
axis and optional position. Enlarge the footprint or move conflicting elements if
validation reports that no staircase fits. Landings must be at least as deep as
the flight width. The stair surface uses **Materials → Frame timber** and receives
no grub. Interior stair geometry appears at LOD0; its collision and floor support
remain active at every LOD, independently of **Show entrance stairs**.

**Enclose stairwell** is enabled by default. It generates a separate interior stair
room with one openable, outward-swinging door from its landing onto each floor.
The enclosure uses the interior wall material; door leaves use **Materials → Doors**
and the building-wide door width/height. **Stair doors** selects plank or frame-and-panel
leaves. Turning the enclosure off retains the open stair layout. An enclosure side less
than one exterior-wall U unit from the inside face of an external wall reuses that
wall, keeping its windows visible. U follows **Dimensions → Exterior walls** and
the assigned wall material's horizontal module. Remaining partitions extend to
meet the reused external walls. A landing door moves to a retained partition when
its original side is shared; placement may reverse the stair flights to keep the
entrance and chimney clear. Exact one-unit gaps retain their partition. Placement reserves
space for the enclosure and the room-side door swing and approach. Layout exports
include stairwell bounds and doorway approach points; `doors.json` identifies these
leaves as `stairwell-door-N`, with `exterior: false`. Their meshes appear at LOD0;
wall and moving-door collision remain active at every LOD.

Automatic stairs prefer the closest usable external wall across both run axes,
then a corner where possible, leaving 40 mm from its inside timber face. Shared
external walls need no extra enclosure-wall setback. The chimney wall and hearth
approach remain reserved for the main rooms. A front-door conflict moves the door
along its existing facade before moving the stairs. Authored entrance offsets,
including overrides, are preferred positions: a clear one stays put; an obstructed
one moves to a clear span with room for the leaf swing and player approach. The
door retains its size, style, hinge and opening direction. Generated ground-floor
windows and exterior entrance steps follow the resolved door; manual window offsets stay
fixed. Saved authoring recipes retain the preferences, while generated prototypes
and packages contain the resolved offsets. Explicit stair positions remain fixed.

Windows beside stair landings move up until their complete sill and casing clear
the platform by 20 mm. If the full window cannot fit below the lintel, or its new
position would overlap another opening, it is omitted. This applies to generated
and manual windows on reused exterior stairwell walls, or beside unenclosed stairs;
other windows keep their original height. Saved recipes retain the authored values.
Door jambs start 20 mm above the floor. Wall caps beneath upper stairwell doorways
end 20 mm below the boards, keeping both sets of faces clear of the floor surface.

**Gothic church** is available under Starting places, through `--preset church`,
in `recipes/gothic-church.json`, and from `BuildingRecipe::gothic_church()`.
Its `church` recipe controls nave width, transept depth and position, twin tower
dimensions, flying-buttress reach and window bay spacing. Width is the overall
transept span and depth is the nave/chancel length. The shared window dimensions
control the pointed lancets; shared door dimensions control the south entrance.
The entrance has two independently openable timber leaves extending to the pointed
arch apex, with no glass fanlight. **Door spring height** sets where the curved
head begins; the arch adds its rise above that height. Each leaf has its own handle
and saved angle, uses the assigned door material, and exports shaped collision
sections in its hinge-local frame. Missing sections retain rectangular collision
for older doors. The closed double entrance is preserved in LOD1 and the LOD2 bake.
Invalid proportions produce a diagnostic before geometry generation.

Each lancet contains two complete pointed lights separated by a central mullion,
with a circular stone eye between their heads and the outer arch. The eye meets
both inner arches and the surround. LOD0 glazing remains clear; LOD1 keeps a
simplified version of the same tracery over opaque glazing, and LOD2 bakes the
pattern. Upper tower windows fit their actual wall height without stretching the
circle into an oval.

The cruciform floor is connected through the crossing, with no internal walls
blocking the arms. LOD0 includes an unfurnished stone interior, solid pointed arches
filled up to the sloping ceiling,
recessed clear glazing, mullions/tracery, paired pointed timber doors, crenellated towers
and pinnacles. Flying buttresses have open circular arches matching the windows,
with uniform stone thickness and slightly outboard supports. LOD1
uses opaque windows and simpler exterior detail. LOD2 preserves the cross, towers
and buttress silhouette with baked stone, roof and Gothic-window bay textures.
The church uses continuous rough stone for walls, ceiling, trim and foundations,
with layered noise driving colour variation and normal-map relief instead of
block joints. The shared recipe is [`rough-stone.json`](../island-rs/texture-recipes/rough-stone.json);
material assignments still override the defaults. The slate roof has closed,
mitred eaves around the nave and transepts. **Structure → Roof overhang** adjusts
the projection; zero removes it. New church presets use a 0.45 m overhang.

Church presets enable the existing two-scale grub and moss shader with a 3 m
height falloff from floor level. Only dedicated exterior stone faces, foundations
and stairs receive it. Inner wall faces, reveals, ceilings, floors, doors and shared
trim remain clean; the mixed LOD2 atlas also stays clean. **Grub** adjusts or disables
weathering without altering the base stone maps.
`layout.church` exports the outline and nave/transept/tower bounds; floor support
follows the cruciform footprint. Foundations also include individual pads under
every outer buttress pier, meeting its underside. Terrain fitting, placement
checks and collision include these pads. Interior and Window inspection views
target the nave and its lancets.

Landing perimeter railings are omitted where the outside wall is within 60 cm
of the landing edge, including its inward timber projection. Wider gaps retain
their guards, and the flight handrails remain attached to the stringers.
Upper-floor stair openings have a continuous timber frame beneath the floorboards,
using the timber material and the wall beam width. Headers and side trimmers meet
at solid corners; exposed joists finish against the outside of this frame. The
rim projects 15 mm into the opening to clear adjacent wall faces, while the bearer
ends still meet its outer faces. Its underside lines up with the joists. A
shallower header beneath the landing edge preserves stair headroom; the cut floor
edges use timber too, with a small top lip completing the visible rim.

**Inspection → Internal stairs** starts beside the ascending flight.
**Inspect storey (0 = ground)** selects an upper room for the Interior view;
**Stair door** frames the door from the room side on the selected floor.
Window inspection follows the selected floor. Walkthrough can climb and descend
the stairs. CLI captures accept `--view stairs`, `--view stair-door` and `--storey 0..5` with the Interior
or Window view. Exported `layout.json` includes floor heights and staircase/landing
metadata, while `support-collision.json` supplies floor and internal stair support
separately from hideable entrance stairs. Each occupied floor has a fireplace,
stone hearth and wall-material surround using the shared chimney dimensions and
materials. They align beneath one roof stack and cap. Stair placement reserves the
chimney wall and a clear route in front of its hearths for the rooms; an enclosure
cannot reuse that wall. Explicit stair positions must satisfy the same clearance.
Fireplace inspection follows **Inspect storey**, including CLI `--storey` captures.

```rust
let mut recipe = island_building::BuildingRecipe::procedural(8.0, 7.0);
recipe.storeys = 3;
// Optional: recipe.staircase.axis = island_building::StairAxis::X;
let building = island_building::generate(&recipe).expect("clear stair layout");
```

Floors and ceilings use built-in **wooden boards**, with 20 cm wide boards,
equal lengths, a regular half-board stagger, narrow bevelled seams and subtle grain.
The reusable recipe is [`wooden-floorboards.json`](../island-rs/texture-recipes/wooden-floorboards.json),
editable in Material Studio. **Materials → Floor** and **Ceiling** recolour the
built-in surfaces independently; each has its own **Choose JSON…** material
selector. Clearing a custom reference restores the built-in boards. Ceiling
boards run across the exposed joists and stay aligned around the chimney
opening. In timber-framed buildings, full board lengths fit two joist bays:
even rows meet alternate joists and odd rows meet the joists between them.
Plank end seams remain visible on the floor; the supporting floor joists are
concealed. Ceiling end seams sit over the exposed ceiling joists. End seams
are filtered across texture pixels so the narrow joints survive preview sampling.
The standalone recipe uses 2.4 m boards. **Regular half-board stagger** in
Material Studio controls this pattern and requires an even number of boards
across the tile. Interior walls and fireplace surrounds retain their plaster finish.
These interior materials appear at LOD0.

Foundations, entrance-stair bodies, treads and landings share one continuous
procedural stone material with grub enabled. The default stone seed, neutral
colour, 512 px maps and grub profile are independent of building seeds, palette
tints, atlas resolution and the optional weathering on other exterior surfaces.
The standard renderer reuses one stone/grub material and its image handles across
independently generated prototypes. The experimental array renderer retains this
shared support material outside its per-building texture banks.
**Materials → Foundation / stairs / tread material** accepts an explicit custom
JSON override for all these surfaces. The retained legacy foundation palette
colour no longer recolours the shared default. Foundations project 8 cm beyond
the walls and extend down by `tan(30°) × footprint diagonal + burial margin`
(default 15 cm). Keep the building level and place its floor top at the highest
terrain point anywhere within the foundation footprint. This covers terrain
whose slope stays within 30° in any direction.

Steps and landings retain their **solid stone slab geometry** on upward faces:
45 mm thick with a small 15 mm nosing and exposed-side overhang. The tread heights
remain unchanged, and caps fit against the threshold/plinth. **Materials → Hearth
slab material** assigns a Material Studio JSON to the fireplace hearth;
clearing that reference restores its joint-free dressed-stone surface from
[`stone-slab.json`](../island-rs/texture-recipes/stone-slab.json). The fireplace's
bottom hearth is one solid piece in the same material, continuous into the firebox.
Foundation and entrance-stair surfaces share the continuous rough-stone finish,
including their upward faces. The fireplace hearth retains its separate dressed
stone material and stays clean. Chimneys and fireplace breasts retain their
existing material settings.

Treads and landing caps are real slabs at LOD0/1 and a thin stone ramp/landing
proxy at LOD2. Their overhang collision and support hide with the stairs. The hearth
remains visible and collidable at LOD0 when entrance stairs are hidden. Exported
`foundation`, `stairs` and `stair_treads` groups bind one foundation material while
retaining separate visibility. Hide `stairs` and `stair_treads` together with
`stairs-collision.json`; `stone_slab` remains the independent clean hearth.

Each exterior door gets a landing and 20 cm rise / 24 cm tread entrance stairs.
Their full length reaches the worst downhill terrain beyond the foundation;
terrain conceals the unused lower steps. **Structure → Generate entrance stairs**
controls whether the asset contains them. **Inspection → Show entrance stairs**
controls the current placement, including its collision and walking support.
This setting is retained by `BuildingInstance::snapshot` / `restore_state`.
**Terrain slope**, **Slope direction** and the **Foundations** camera provide
an inspection preview without changing the building recipe.

**Structure → Stair direction → Along wall** adds a turning landing and runs
along the outside of the wall toward the farther corner. Moving a door past
the wall midpoint automatically reverses the flight; a centred door uses the
right-hand side when viewed from outside. **Outward** remains the default for
older recipes. The JSON option is `foundation.stair_run: "along_wall"`.
Both choices retain the full 30° terrain reach and the placement visibility switch.

LOD0 and LOD1 retain solid steps; LOD2 uses a 24-triangle stair silhouette.
Foundation and stair surfaces share baked stone maps independently of the
wooden floor. Exported material groups keep stairs independently hideable;
`stairs-collision.json` must be switched with them. `foundation.json` describes
the buried bounds, and `stairs.json` records each doorway's reach and step sizes.

**Openings → Door style → 18th-century panels** selects a Georgian-style
six-panel frame-and-panel leaf for that door. It has solid stiles and rails,
small upper panels, taller middle/lower panels, moulded borders and raised fields
on both faces. Grain follows the stiles and panels vertically and the rails
horizontally when a material is assigned. Door colour/material, knob, hinge,
clearances and opening action use the existing controls. LOD1 keeps a simpler
closed exterior; LOD2 bakes the panel design. Older recipes keep **Plank** doors.
The recipe value is `"door_style": "frame_and_panel"` on the opening.
Both door styles seat within the lining, with the inside face flush with the
room-side frame and the hinge at that face so the leaf swings clear of the jamb.
The design draws on the [Georgian Group's historic front-door examples](https://georgiangroup.org.uk/wp-content/uploads/2024/07/GGM_Brooking_09_Front_doors.pdf).

**Structure → Wall construction → Wattle & daub** adds pale plaster infill with
timber posts, rails, braces and gable framing. Timber defaults to 14 cm wide,
projecting 25 mm from the plaster, with adjustable width, projection and bay
spacing. Selecting a wall style restores its wall colour and built-in surface.
**Materials** provides a separate timber colour and optional JSON material;
the default wood grain follows each member. The reusable plaster recipe is
[`wattle-daub.json`](../island-rs/texture-recipes/wattle-daub.json), and
[`wattle-daub-cottage.json`](recipes/wattle-daub-cottage.json) is a complete example.
Framing is clipped around openings and kept within the roof profile. LOD0 has
full-depth members visible on both sides of the wall. LOD1 keeps their
exterior fronts; LOD2 bakes the same pattern with filtered edges into its minimal
shell. Older recipes retain plain walls.
Each LOD0 corner is one closed rectangular timber through both walls, with
vertical grain and collision. Corner sections include the wall thickness and
beam width, fitting beside nearby opening casings; rails and braces butt into
them. Timber roof edging overlaps beneath the smoothed thatch to hide the plaster
joint. The ceiling meets the top of the wall-top rail where roof clearance allows,
with upper plaster, partitions and the fireplace extended to meet it. This leaves
the full top plate visible. LOD0 adds exposed joists across the room between the
east and west window walls, evenly spaced independently of stud and opening
positions. The bay-spacing setting controls their maximum centre spacing, with
equal clear gaps between joists and at the end walls. Their width and depth
follow the timber width, and their undersides meet the bottom of the top plates.
Joists share the timber colour/material with grain along the span, include
collision and stop at the chimney breast to keep the flue clear.
Horizontal timbers join the upright posts directly above and below each window
and above each door, fitted against the casing and sill with grain along the beam.
**Inspection → Corner** provides a close view of both joins.

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
they use the embedded [`rough-stone.json`](../island-rs/texture-recipes/rough-stone.json)
recipe: continuous noise-based stone with normal-map relief over a 2 × 2 m tile.
The chimney palette swatch recolours the stone; a custom material recipe supplies
its own colours. Choose any shared masonry or brick recipe with **Choose JSON…**;
**Clear** restores the built-in rough stone. The soot lining keeps its separate
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
`east`, `west`, `entrance`, `extension`, `fireplace`, `roof-end`, `roof-under`, `eaves`, `window` and `corner`; `--capture-ui` includes the HUD. Captures wait for
valid geometry and rendering to settle, then exit after the PNG is written.
Interior/entrance inspection can intentionally pin LOD0 even when a farther LOD
is requested. Use exterior facade views for forced LOD comparisons.

The **Grub** tab adds optional shader weathering without changing the material
recipes or their texture maps. Two separately contrasted layers of seeded noise
combine broad dirt patches with finer breakup; both sizes and the fine layer's
contribution are adjustable. The full noise contrast is retained at the building
floor, so blotches remain visible on the walls above it.
Moss patches favour upward-facing surfaces using the final normal-mapped normal.
The height blend uses the **building's floor**, independently of terrain height.
Neutral noise has 50% strength there, increasing below it and fading above it.
Individual blotches vary around that midpoint. Grime and moss strengths,
height falloff and both colours are adjustable. Weathering is restricted to
foundations, stairs/treads, thatch, exterior wattle-and-daub panels and dedicated
church exterior stone. Timber,
joinery, doors, glass, chimneys/fireplaces and all interior surfaces keep their
original materials. Shared geometry is excluded entirely, with no side masks.
The mixed LOD2 facade atlas also stays clean to protect its baked timber and
joinery; dedicated foundations, stairs and thatch remain eligible at LOD2.
Existing recipes keep the filter disabled.

Portable exports include `grub-filter.json` with settings, uniform vectors and
coordinate conventions, plus `grub-filter.wgsl` with the same noise/masking
functions used by the renderer. The exported maps and LOD2 atlas stay unfiltered;
importers must apply the shader to reproduce enabled weathering.

The default library has no Bevy dependency. Consumers use `generate` or
`generate_with_base`, then enable `bevy` for `compile_building`, `spawn_building`
and `BuildingRuntimePlugin`. Compiled assets are shared through `Arc`; each
placement has one scene root and its own retained doors. Attach an explicit
`BuildingObserver` at feet position and, for a detached camera, retain a
`BuildingOccupant` for the player. Runtime consumers own terrain placement and
movement; collision queries do not depend on active visual LOD.

To render the grub filter, also register `BuildingGrubPlugin` after Bevy's
render/PBR plugins and use `compile_building_with_grub`, passing
`Assets<GrubMaterial>` alongside the standard assets. This shares the existing
image handles and supplies weathered materials at all three LODs. The original
`compile_building` API remains available for standard PBR consumers.

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
compilation alone does not establish visual or interactive quality. The Bevy
adapter supports standalone inspection; the coastal-town Unity adapter and
prefab-baking workflow are documented below and in the shared runtime package.

## Coastal towns without an engine

`town::generate_town(&TownRecipe, &TownInput)` is a reusable Rust API; no Unity,
Bevy, renderer, filesystem output or terrain mutation is required. The supplied
height grid and river lines are borrowed during generation. Outputs own shared
building shells, instance poses, one combined support mesh and clearance areas.

```rust,no_run
use island_building::town::{generate_town, TownInput, TownRecipe};

let recipe: TownRecipe = serde_json::from_str(&std::fs::read_to_string(
    "recipes/towns/coastal.json",
)?)?;
let input: TownInput = serde_json::from_str(&std::fs::read_to_string(
    "coastal-tile-input.json",
)?)?;
let town = generate_town(&recipe, &input)?;
for instance in &town.instances {
    let shared_shell = &town.prototypes[instance.prototype_index].building;
    // Place this shell at instance.position, rotated by instance.yaw.
    println!("{} at {:?}", shared_shell.recipe.name, instance.position);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

All coordinates use native metres with XY horizontal and +Z up; yaw is radians
about +Z. `TownInput` contains `tile_seed`, `sea_level`, `terrain` and `rivers`.
`HeightGrid` has `origin: [x,y]`, one positive `spacing`, `width`, `height` and
row-major `heights[y * width + x]`; sampling is bilinear without extrapolation.
`RiverPolyline.points` run source to mouth. Its `widths` contains full channel
widths per point, or can be empty for a 3m default. Tributaries may end on another
polyline. Optional `join` contains an authoritative downstream river index for
confluences meeting at a channel edge rather than its centreline. Without it,
coincident endpoints are matched geometrically. Selection compares complete source-to-sea route lengths, counting a
shared downstream tail once, rather than adding tributary lengths together.
A river must reach the sampled sea level; inland endings and riverless tiles
produce an empty town.

`TownRecipe.catalogue` entries have a unique `id`, inline `building` recipe,
`min_count`, `max_count` and `placement` (`"centre"` or `"other"`). Counts use an
inclusive deterministic range derived from the tile seed and entry ID. Each
entry has one stable building variant shared by all its copies; changing copy
count never changes that entry's prototype. Centre entries are attempted first,
followed by other entries around the first accepted site. The default catalogue
contains one cathedral, 5–12 cottages and 2–4 workshops, using 128px building
atlases and one shared 512px foundation-stone material.

`TownSettings` controls mouth-site and town radii, separation, channel and
vegetation margins, entrance approaches, maximum slope and actual foundation depth, minimum
dry height, sampling spacing and bounded search attempts. Unsafe or crowded
copies are skipped (`skipped_count`); counts describe requests rather than a
promise to flatten terrain. Dense footprint and approach samples reject steep,
submerged or unavailable terrain, channel crossings and overlapping placements.
The default limits slope to 14 degrees and actual foundation masonry depth
(including burial) to 4m. Floor height is fitted to occupied foundation regions,
so empty church recesses and roof margins do not raise the building or consume
its foundation depth budget. Entrance approaches retain dry-ground and slope
checks. Support sampling uses the final resolved building recipe, including doorway
sizes and staircase entrance offsets. `generate_town_with_base` additionally
accepts a material-reference base directory and cancellation callback. A fixed
8-million-site sampling budget also bounds pathological search workloads.

`TownOutput.supports` is one world-space `BuildingMesh`; `support_material` owns
its shared stone maps. `supports_grub_positions` retains each vertex's original
building-local position for matching floor-relative stone weathering after
combination. `support_colliders` and `stair_colliders` are conservative world
axis-aligned bounds. Shell prototypes exclude baked foundations and entrance
stairs, so consumers should instantiate the fitted supports separately.
`clearance` contains rotated XY rectangles covering shells, entrances and fitted
supports plus the configured margin. Consumers must remove decorations against
these rectangles before exporting or instantiating their terrain; the pure API
returns the areas without modifying the supplied terrain grid.

```sh
cargo test --manifest-path island-building/Cargo.toml --test towns --no-default-features
```

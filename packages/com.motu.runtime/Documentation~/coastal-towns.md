# Coastal towns

Coastal request factories expose **Coastal Towns**. Towns are enabled by default;
radial islands are unaffected. Leave the catalogue empty to use the packaged
cathedral, cottages, workshops, and smaller houses. Copy counts supply
candidate buildings, and the usable street frontages determine how many fit.

Create a catalogue with **Assets → Create → Motu → Town Recipe Catalogue**.
Assign Building Studio recipe JSON TextAssets and a stable unique ID to each
entry, then set its minimum/maximum copy count and Centre/Other placement.
Bind each referenced material recipe path to its JSON TextAsset in the catalogue
Material Dependencies list. The adapter captures these dependencies and stages
content-addressed files for the native call, so player builds do not depend on
developer filesystem paths.

The catalogue Inspector also provides **Bake Preview Building Prefabs** and an
explicit preview seed. It saves real reusable `.prefab` assets and persisted
shared meshes, materials and textures under a unique `Assets/Generated/Towns`
folder. Preview prefabs contain building shells, LODs, structural collision and
hinged doors. Town-specific terrain-fitted foundations/stairs remain runtime
geometry. Preview baking does not change runtime tile-derived building seeds.

TerraNullius includes seed-42 preview prefabs at
`Assets/Generated/CoastalTownBuildings`, with persisted shared meshes/materials
and PNG texture maps. Their README explains regeneration and runtime ownership.

The catalogue also exposes native site radius, building and river margins,
maximum slope and foundation depth (including burial). Requested minimums are
targets. Native generation skips copies when no safe dry placement exists.
A coastal section without a river reaching the sea produces no town.
Regenerate or reload after catalogue edits.

Coastal Towns enables roads by default. Rust starts with one square ring around
the central building. When the catalogue needs more frontage, the planner tries
additional concentric rings and keeps the smallest layout that fits the most
requested copies. Terrain and the road geometry budget bound growth. The town
radius setting applies to placement with roads disabled.

Cross roads connect the rings at their side midpoints. Each junction matches the
width of the street entering from each axis, keeping the central square clear
for the building and its walking approach. Water, steep terrain, or excessive excavation
can interrupt a ring street. The sections that reach the town centre stay in use.
A cross road between rings requires a complete span and a junction at each end.
An outward access road must reach its planned exit. Where a river interrupts a
street and both banks can support landings, the plan records a candidate for a
future bridge.

Roads have a level cross-section and a constrained grade along their length.
The same native grade drives the road surface and terrain collision, with
shoulders that blend into the natural slope. Buildings use the frontage of a
finished street. Rust fills each frontage with larger footprints first and
fits smaller houses into the remaining spaces. The default minimum gap is
1.5 m between occupied footprints. Decoration clearance can overlap without
spacing the buildings further apart. The resolved entrance points toward the
street and the stairs and approach are checked for walkability.
Road controls cover ring spacing, road widths, shoulder width, grade, and cut
depth. Disable roads to use the earlier placement style.

Unity snapshots the catalogue text when constructing the generation request.
Rust selects the longest source-to-sea river, plans the town, derives one
prototype seed per recipe from the canonical coastal section seed, fits per-copy
foundations and stairs, and removes terrain decorations from the resulting
building and road clearances. Snapshots retain the original terrain and
decorations. Roads and clearance are applied on the resident island before
terrain and decoration exports. The finished road surface has a shared material
and uses terrain collision.

The Unity adapter shares prototype meshes, materials and textures across copies.
Each building has three native LODs, native structural box colliders and hinged
doors. `TownDoor.Toggle()` and `SetOpen(bool)` are public host interaction hooks.
Terrain-fitted foundations/stairs use one native combined mesh, one material and
one MeshCollider. Shared stone maps and native local weathering coordinates keep
the foundation appearance consistent across the combined geometry.

Both `libmotu.dylib` and `libmotu_building.dylib` must match this package; restart
Unity after updating native binaries. CoastlinePlayground exposes the enabled
toggle, generated building count and **Visit town** camera action. OpenSeaWorld
uses the same catalogue through `CoherentIslandFactory`.

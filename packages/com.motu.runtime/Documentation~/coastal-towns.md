# Coastal towns

Coastal request factories expose **Coastal Towns**. Towns are enabled by default;
radial islands are unaffected. Leave the catalogue empty to use the packaged
cathedral (one Centre), cottages (five to twelve Other), and workshops (two to
four Other).

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

The catalogue also exposes native site radius, building/river margins, maximum
slope and foundation depth (including burial). Requested minimums are targets: native generation skips
copies when no safe dry placement exists. A coastal section without a river
reaching the sea produces no town. Regenerate/reload after catalogue edits.

Unity snapshots the catalogue text when constructing the generation request.
Rust selects the longest source-to-sea river, plans the town, derives one
prototype seed per recipe from the canonical coastal section seed, fits per-copy
foundations and stairs, and removes terrain decorations from the resulting
clearance rectangles. Snapshot caches retain the original terrain/decorations;
the requested town clearance is reapplied before decoration exports.

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

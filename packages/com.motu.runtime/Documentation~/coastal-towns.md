# Coastal towns

Coastal request factories expose **Coastal Towns**. Towns are enabled by default;
radial islands are unaffected. In each coastal section, every river outlet that
reaches the sea gets a settlement. The outlet closest to the midpoint between
the two outermost outlets along the coast gets the capital. All other outlets
get villages.

Assign one asset to **Capital Catalogue** and a separate asset to **Village
Catalogue**. Every village uses the same village asset. Existing catalogue
assignments carry over to **Capital Catalogue**. With both fields empty, the
capital uses the packaged cathedral, cottages, workshops and smaller houses.
Villages use a smaller packaged catalogue of workshops, cottages and houses.
Copy counts supply candidate buildings. The available sites and frontage
determine how many fit in each settlement.

Create a catalogue with **Assets → Create → Motu → Town Recipe Catalogue**.
Assign Building Studio recipe JSON TextAssets and a stable unique ID to each
entry, then set its minimum/maximum copy count. Centre/Other placement applies
when roads are disabled.
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

Each catalogue also exposes native site radius, building and river margins,
maximum slope and foundation depth (including burial). Requested minimums are
targets. Native generation skips copies when no safe dry placement exists.
A coastal section without a river reaching the sea produces no town.
Regenerate or reload after catalogue edits.

Coastal Towns enables roads by default. The planner starts with one square ring
around the largest footprint that fits near the river mouth. It fills both sides
of the street, including usable plots inside the square. When copies remain,
it tries another ring and rechecks existing entrances against the updated road
grade before placing the remaining copies. Terrain and the road geometry budget
bound growth. The town radius setting applies to placement with roads disabled.

Cross roads connect the rings at their side midpoints. Each junction matches the
width of the street entering from each axis. Water, steep terrain, or excessive excavation
can interrupt a ring street. The sections that reach the town centre stay in use.
A cross road between rings requires a complete span and a junction at each end.
An outward access road must reach its planned exit. Where a river interrupts a
street and both banks can support an arched bridge, the linked street network
retains the graded approach on each bank.

Roads have a level cross-section and a constrained grade along their length.
The same native grade drives the road surface and terrain collision, with
shoulders that blend into the natural slope. Buildings use the frontage of a
finished street. For each ring, the planner works from largest to smallest
footprint. It divides each street side into blocks sized for the current recipe.
Each copy starts at a seeded random block and checks the others in order if the
first block fails. It tries other offsets within the blocks to fill gaps.
The default minimum gap is
1.5 m between occupied footprints. Decoration clearance can overlap without
spacing the buildings further apart. The resolved entrance points toward the
street and the stairs and approach are checked for walkability.
The capital and villages share the Coastal Towns road controls for ring
spacing, road widths, shoulder width, grade and cut depth. Disable roads to
place buildings on nearby dry sites.

Unity captures both catalogue assets when constructing the generation request.
Rust plans the settlements, derives building prototypes from the canonical
coastal section seed, fits per-copy foundations and stairs, and removes terrain
decorations from the resulting building and road clearances. Snapshots retain
the original terrain and decorations. Roads and clearance are applied on the
resident island before terrain and decoration exports. Both catalogues
contribute to the graded horizon cache key. The terrain shader blends the
`island-rs/texture-recipes/Road.json` material using road coverage on the
streamed terrain vertices. Fallen Stones forms a gravel shoulder between the
paving and surrounding ground. The close terrain uses the recipe's height map
and physical height range for parallax, and height-blends the paving into the
gravel. The road profile clears grass from the road and drives gravel footsteps.

**Connect Settlements** grows one cart-track network from the capital. It visits
villages in order of distance from the capital. Each branch starts at the closest
connected settlement. A* compares usable cross-street ends, outermost ring
corners and ring dead ends at both settlements in one search. A dead end can
connect straight ahead or from its outer curb. If
none can reach the destination, the planner tries the next connected settlement
in distance order. Routes follow the terrain mesh and weigh both along-track grade and
cross-slope at the wheel lines. Their curves are checked against the original
ground. Fallen Stones fills each rut and grass or soil can show through the
centre. Where the wheel lines are at different heights, the higher rut is cut
towards the lower one by at most 30 cm. The cut feathers across the rut and
fades into paved roads, bridge landings and section seams. Close terrain and
collision include the depression. Distant terrain retains the painted ruts.

A route can include a stone bridge over a river or ravine. The report records
the two landings, where the dry tracks meet the bridge deck. Game code can read
the landings through `IslandGenerator.TryGetCartTrackBridgeCandidate`.
Adjacent coastal sections meet at shared dry portals with matching wheel
spacing. Each bridge belongs to the section containing both of its landings.

The same native generator fits town and cart-track bridges. The deck runs
straight between the two finished bank elevations. Cart tracks turn on the
dry approaches to meet the deck in line. The fitter lays out segmental arches
around the river channel and extends each pier and abutment into the ground
beneath its footprint. The deck, parapets and arches share the churches'
weathered exterior stone and render as one mesh per detail level. Unity
installs a continuous collider and includes the deck in navigation. The
distant mesh retains the open arch silhouettes.

### Capital pier

The coastal capital has one pier where the closest usable open-ocean shoreline
can support an Explorer berth. The shared Rust planner checks the resident seabed
under the whole berth and its approach. It currently fits a 42 by 16 metre berth
with at least 5 metres of water depth. The timber piles follow the seabed, and
an existing-vertex gravel path links the stone landing to the town streets.

The pier exports separate meshes for rendering, collision and walking. The same
plan supplies the berth position and heading, the approach waypoint and
deterministic cargo prop points. The working deck is clear by default.
Create an optional catalogue with **Assets → Create → Motu → Capital Pier Clutter Catalogue**
and save it at `Assets/Resources/Towns/CapitalPierClutterCatalogue.asset`. Add
prefabs with the `cargo` kind. Place each prefab's origin at its feet and align
its +Z axis along the deck. Keep its rendered footprint within 0.48 m on either
side of its origin and 1 m forward or back. The adapter picks a fitting prefab
deterministically for each point and leaves the walking surface clear.

### Wheat fields

The town planner finds room for fields in the finished street layout after
placing buildings. The resident terrain trims those plots around steep or wet
ground, roads, footpaths, cart tracks and bridge approaches. A field coverage mask
suppresses grass and exposes dirt, and the town's decoration clearance removes
rocks and wild vegetation from the accepted plots.

LOD0 terrain tiles draw instanced wheat clumps in seeded rows that share the
island wind. LOD1 and LOD2 tiles carry raised canopies clipped to their finished
terrain, with mottled colour and row-aligned texture coordinates shared across
tile seams. The clumps and canopy dissolve during the tile handoff. The canopy
fades out as the view approaches the LOD3 horizon.

The Unity adapter shares prototype meshes, materials and textures across copies.
The scene hierarchy groups buildings by capital and village.
Each building has three native LODs, native structural box colliders and hinged
doors. `TownDoor.Toggle()` and `SetOpen(bool)` are public host interaction hooks.
Terrain-fitted foundations/stairs use one native combined mesh, one material and
one MeshCollider. Shared stone maps and native local weathering coordinates keep
the foundation appearance consistent across the combined geometry.

Both `libmotu.dylib` and `libmotu_building.dylib` must match this package; restart
Unity after updating native binaries. CoastlinePlayground exposes the enabled
toggle, generated building count and **Visit town** camera action, which focuses
on the capital. OpenSeaWorld exposes both catalogues through
`CoherentIslandFactory`.

# Island caves

Caves are generated after the final terrain, erosion and river pass. Each accepted
cave has a cliff-foot entrance, a gently winding passage and a terminal chamber.
An island can legitimately have no caves: a requested count is a maximum, and
unsafe entrances are rejected.

## Use in Unity

`OpenSeaWorld` enables caves on its `CoherentIslandFactory`. Other profiles remain
disabled by default. Expand **Cave Settings** on the factory to configure the
entrance, approach, rock cover, passage, chamber and two layers of shape noise.
Dimensions are metres and slopes are degrees. The initial passage layout has a
level floor; bounded noise adds floor variation.

Settings apply to newly generated island requests. Change them before entering
Play Mode. They are copied into each request and included in the snapshot cache
key, so a changed configuration regenerates the island automatically. Existing
installed geometry does not change when settings are edited during play.

Scripts use the same settings:

```csharp
using Motu.Islands;

void ConfigureCaves(CoherentIslandFactory factory)
{
    factory.CaveSettings.Enabled = true;
    factory.CaveSettings.MaximumCaves = 2;
    factory.CaveSettings.MinimumFaceHeight = 8f;
    factory.CaveSettings.RoofCover = 4f;
    factory.CaveSettings.LengthMin = 30f;
    factory.CaveSettings.LengthMax = 60f;
}
```

The ship remains the normal starting controller. The on-screen stats show the
cave count on the current island and across all loaded islands. When a cave is
available, **Teleport to cave mouth** takes you to the nearest mouth, facing into
the passage in walking mode. Press **Tab** to release the cursor before clicking.
This works from the helm, flying camera or overview, including caves on dormant
loaded islands. Hover the button for the destination square and distance, or the
cave count for the current island's rejection diagnostics. With no available
caves the stats show zero and the button is hidden. Generation stats also include
the accepted cave count.

The editor alternatives are **Motu > Caves > Visit first available entrance**, or
selecting an island's `Caves` object and using **Walk from entrance (debug)** /
**Visit chamber (debug)**.
These switch to the existing first-person camera and controller. The inspector
shows candidate rejection counts; selecting the object also shows entrance and
chamber markers. The menu searches currently loaded islands, so it may report
that none have caves. Stop Play Mode to return to the normal ship startup.

The native plugin has additional entry points. After updating an already-open
Unity editor, save any work you want to retain and restart Unity to load the
rebuilt `Assets/Plugins/macOS/libmotu.dylib`. If keeping an unsaved version of
`OpenSeaWorld`, check **Cave Settings > Enabled** after reopening it.

## Geometry and collision

The original terrain render mesh stays in use, including above the entire
underground passage. Only a small arched entrance prism is subtracted. Triangles
outside that aperture keep their original surface and material data. Local
edge subdivisions make clipped and neighbouring triangles agree, while keeping
them in the normal terrain and fur-grass rendering paths.

Each render LOD clips its own entrance triangles and builds a connecting lip
from those exact cut edges to the common tunnel throat. This avoids both a
whole-roof replacement and a gap between a coarse terrain LOD and the entrance.
The arch uses 48 segments, with a wider upper rounding (30 percent height flare)
and a smaller side flare (18 percent width). The lip uses a subdivided cubic curve tangent to
the original terrain and to the passage. Smooth normals blend across the join;
the rendered lip and its collider use the same curve. Shared cut endpoints are
resolved once before meshing, including floor/wall corners and differing source
normals. Triangle winding follows the cut boundary rather than lighting normals.
A local stitching pass makes shared positions bit-identical and splits both
sides of T-junctions. Its bounds include the entire affected source triangles,
including clipped fragments beyond the aperture. The lip and passage use shared
throat anchors and matching edge subdivisions. Original roof geometry is kept.
The lip uses the terrain material; grass and decorations are excluded only in
the low entrance/approach area. Vegetation above the cave remains eligible.

The separate cave mesh contains only the underground passage and terminal
chamber. Arched cross sections follow the accepted route, with broad/fine wall
noise and bounded floor variation. The entrance lip has a separate collider
built from the authoritative terrain; its renderer stays off because the normal
terrain LOD already renders the lip. There is no duplicate exterior roof mesh.

The passage uses the same configured stone colour, island tint and procedural
rock-normal detail as the exterior cliff. Authored dirt textures still apply to
the floor; wall stone is not replaced with the unrelated top-down rock recipe.
The shared throat ring uses matching normals on both meshes, and ambient
darkening begins after that ring rather than stepping at the material boundary.

Placement still checks a walkable approach, cliff height, river separation and
rock cover around the full passage/chamber envelope. Final standing-capsule
checks include the entrance lip and passage. The sampled height field is retained
for clearance and ambient calculations, not rendered as replacement terrain.

The exterior TerrainColliders have no holes. While the walking player is on cave
or entrance ground, `CaveCollisionScope` disables exterior colliders and keeps
the cave/player colliders active. Newly streamed terrain colliders are disabled
immediately; other newly created exterior colliders are caught before the next
player movement. Leaving the cave, teleporting, changing cameras/fly mode, or
disabling the player restores only colliders that the scope previously disabled.
Walking on the original roof does not enter cave mode because its ground is well
above the cave floor. This is a single-player collision policy.

Interior rendering uses the island stone palette and triplanar dirt from the
island texture array, with diffuse/cloud/fog lighting, direct shadows and ambient
attenuation with cover and distance from the mouth. Press **T** in walking or
flying mode to toggle the camera torch. Its range, intensity, beam angle and key
are configurable on `FirstPersonController`. The cave shader supports shadowed
local lights independently of sunlight and cloud cover. Separate fine material normal
maps remain a future visual refinement.

The complete cave stays resident with its island. Unloading it disposes its
meshes, colliders and material. Native generation finishes before cancellation
can release its handle; managed installation checks cancellation between meshes.
Flooded caves and underground rivers are outside this version.

## Limits and persistence

- At most four caves, 128 mesh chunks and 250,000 cave triangles per island.
- At most 4,096 entrance candidates and 131,072 exterior samples per cave.
- Passage length at most 120 m; target mesh spacing 0.25–1 m.
- Route search radius at most 64 m; dimensions/noise are validated before work.
- Geometry and height-sampling budgets are validated before installation.
- Native snapshot version 7, Unity cache-key schema 7, cave algorithm revision 7.
- Older native generation entry points still create caves-disabled islands.
- Exported cave mesh buffers own their data independently and use `ReleaseMesh`.

The old fixed-volume `caves/voxels.rs` experiment is no longer compiled. It has
been left on disk as an unrelated experimental source; production cave code
allocates no such volume.

## Reproduce validation

Generate the synthetic cliff fixture, then a real island snapshot:

```sh
MOTU_CAVE_FIXTURE_OUTPUT=/tmp/motu-cave-fixture.json \
  cargo test --manifest-path island-rs/Cargo.toml --no-default-features --lib caves::
cargo run --manifest-path island-rs/Cargo.toml --release --bin island-cave-probe -- \
  --seed 5 --terrain-size 128 --output /tmp/motu-cave-check
```

Run Unity EditMode tests with `MOTU_CAVE_FIXTURE_OUTPUT` set to that fixture and
`MOTU_CAVE_SNAPSHOT=/tmp/motu-cave-check/5.motusnapshot`. Select `Motu.Editor.CaveTests`
and `Motu.Editor.RuntimeContractsTests`. The two geometry tests explicitly skip
when their external native fixtures are not provided. Optional
`MOTU_CAVE_SCREENSHOT_DIR` saves entrance, roof and interior renders.

The probe also accepts `--count` for a deterministic seed corpus and reports
elapsed generation time, accepted caves, rejection counts, chunks and triangles.
Build with `--features profiling` to include `caves.generate` and `caves.mesh`
stage timings in the native profiling output.

Hairline seam revision 7 validation: eleven native cave tests and five Unity
cave tests passed. Exact-position edge checks cover the clipped surface and its
join to the passage; original roof surface coverage and walking remain checked.
Strict production Clippy passed. A close render translated 18 km reproduced
the dotted fractures before the fix and showed them closed afterward. Sample
seed 5 generation at terrain size 128 remained about 2.8 seconds.

Upper-lip and stone revision 6 validation: ten native cave tests and five Unity
cave tests passed, with no skips. Tests cover throat normal/ambient continuity,
arch chord error, the existing closed-seam and traversal checks, and a pixel
render proving the stone palette remains effective with recipe textures enabled.
Strict production Clippy passed; final close and side entrance renders were
inspected in the isolated editor project.

Seam repair revision 5 validation: ten native cave tests and four Unity cave
tests passed. The new regression checks every shared curve edge, including a
floor/wall corner with differing normals and tile-local corner classification.
Strict production Clippy passed. Close/front/side renders were checked, alongside
a before/after render using existing cached island terrain. Older runtime cache
keys regenerate islands with the corrected entrance collider.

Rounded entrance revision 4 validation: nine native cave tests and four Unity
cave tests passed, with no skips. Strict production Clippy passed. Entrance and
roof renders were inspected in the isolated editor project. Seed 5 at terrain
size 128 generated one cave with 7,023 passage/entrance-collider triangles.

Entrance-only revision 3 validation on macOS / Unity 6000.5.6f1:

- Native library suite: 386 passed, three existing ignored tests; focused cave
  suite: eight passed. Strict production library/binary Clippy passed.
- Unity cave and ship-view tests: eight passed, none skipped. These cover the
  preserved roof at every LOD, generated entrance rendering and traversal,
  suspending exterior collision inside, newly streamed colliders, and restoration
  on walking out or leaving walking mode. Previously disabled colliders stay disabled.
- Generated entrance and roof renders were inspected in an isolated editor project.
  Live-editor grass appearance still needs checking after restarting Unity.

Earlier whole-roof implementation validation (historical, superseded geometry):

- Native library suite: 383 passed, three existing ignored tests, before the
  additional thin-roof/ridge test. Final focused cave suite: six passed.
- Strict Clippy passed for the library and binaries with and without default features.
- Unity cave and runtime contract tests: 15 passed, none skipped, including real
  snapshot save/load equality, exported buffer lifetime, a generated entrance
  traversed against real heightfield tiles, synthetic roof traversal, translated
  island queries and disposal.
- The final three cave tests passed again with the published native build, including
  checking that all three terrain LOD exports leave the cave footprint open.
- The macOS development player build passed with the cave-enabled demo scene.
  The live editor was not restarted or its unsaved scene saved.
- Seed 5 at terrain size 128 produced one cave, 32 mesh chunks and 90,458 triangles;
  generation took about 3.1 seconds on this machine. Its 135,995 unique mesh edges
  had no unmatched interior edges. These are sample measurements, not a target
  hardware budget or a guaranteed cave frequency.
- The broader Rust run passed 33 generation integration tests but failed
  `rivers_are_continuous_flowing_terrain_submeshes_with_waterfalls`. The same river
  UV assertion fails on the unchanged `3716849` baseline. All-target strict Clippy
  also finds an existing float-equality assertion in runtime material tests;
  production library/binary Clippy is clean.

The original design and later expansion ideas remain in
[ISLAND_CAVES_PLAN.md](ISLAND_CAVES_PLAN.md). Branching caves, mining, flooded caves,
independent chunk LOD and normal ship disembarkation are not part of this version.

To repeat placement on an existing cached island without rerunning erosion or
changing the snapshot:

```sh
island-rs/target/release/island-cave-probe --snapshot /path/to/island.motusnapshot
```

The probe prints the original rejection counts and options, then the regenerated
counts and cave-only time. `--cave-options /path/to/options.json` overrides the full
native options block; `--output /tmp/cave-inspection` exports regenerated cave JSON
only. Old snapshots retain their original settings unless overridden. Unity's
revision-7 cache key regenerates older islands automatically; restart
the editor to load the new native plugin first. An unsaved scene can retain the old
512-candidate value; use **Cave Settings > Candidate Limit = 2048** in that case.

`GeneratedCaveExteriorUsesTerrainMaterial` verifies that no copied roof is exported
and that the passage uses the island texture array. Set
`MOTU_CAVE_GENERATED_SCREENSHOT_DIR` to export mouth and cliff views. Unity tests
also exercise entering/exiting with complete heightfields, streamed exterior
colliders, teleport/fly-mode restoration, and walking above the cave.

The rounded entrance architecture uses revision 7 and snapshot version 7. Restart
Unity to load the rebuilt plugin. Older cached islands regenerate automatically
under the new revision; their old replacement meshes are not reused.

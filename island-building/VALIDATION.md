# Building Studio validation — 10 November 2026

Implementation and checks were performed in the main Island-experiment checkout
on branch `main`. The initial Building Studio and door/material follow-ups were
committed and pushed as `04924a4`; the chimney/fireplace extension remains
uncommitted for review. Existing Unity weather/wetness work
and the pre-existing crash report were left untouched.

## Automated checks

- New crate: **30 library tests and 13 core integration tests passed** with the
  complete `studio` feature enabled. Coverage includes true openings, valid
  geometry, both roof families, every facade and hinge side, structural door
  swings, floor support, independent door states, occupied LOD residency,
  document conflicts/history, and portable export regeneration.
- Planet viewer: **194 tests passed**, including actual controller movement
  against closed/open doors and ceilings, metre-scale placement at multiple
  radial orientations, footprint obstruction, saved-scene identity checks, and continued frame updates during blocked file I/O.
- Strict Clippy passed for the new crate with all targets and for the Planet
  production binary. Planet all-target Clippy also reports existing test-only
  float comparisons, numeric literal formatting, and one long forest test in
  unchanged forest test code; those were preserved.
- New-crate formatting, changed Planet-file formatting, and the focused Git
  whitespace check passed.

Commands, from the repository root:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path island-building/Cargo.toml --features studio --lib --test core
CARGO_TARGET_DIR="$PWD/target" cargo clippy --manifest-path island-building/Cargo.toml --features studio --all-targets -- -D warnings
cargo test -p planet-bevy
cargo clippy -p planet-bevy --bin planet-bevy -- -D warnings
cargo fmt --manifest-path island-building/Cargo.toml --all -- --check
```

The crate's independent `Cargo.lock` and the root workspace lock are included.
Default library builds contain no Bevy dependency; runtime and studio are
separate optional features.

## Rendered and live acceptance

Actual Metal captures were inspected for all three hut LODs, closed/open entrance
doors, windows from inside and outside, daylight and low-light empty interiors,
the cottage partition doorway, and the editor HUD. This review caught and fixed
missing LOD1 frames, missing baked roof seams, inappropriate default mineral
flecks, roof/base shell gaps, and an atlas-height mismatch.

The ad-hoc-signed debug `Building Studio.app` was launched and operated through
the native UI. Walking into the closed door stopped movement; keyboard E opened
the leaf; the player entered through the doorway. Selecting LOD2 inside retained
LOD0. Returning to exterior inspection displayed LOD2, and restoring LOD0
restored the open leaf. The app was left in its detailed Auto inspection view.

Planet completed an offscreen rendering smoke run and a native live check on a
flat, dry 2000 m test planet. H placed the built-in hut at metre scale; the closed
door stopped movement; E opened it; the explorer entered the empty interior.
Save scene wrote the placement and open door state to a temporary scene file.
Load scene completed, and revisiting the entrance showed the restored open leaf.

Live review also exposed a synchronous recipe read blocking a frame inside the
OS file-open call. Recipe reads and scene reads/writes now use an owned I/O task;
Update only polls completion. A gated-worker regression checks continued updates,
success/error completion, and single delivery without touching the filesystem.
The final live placement/persistence check used the built-in recipe and a /tmp
scene path. Unity integration remains a later milestone.

## Repeated instances

A headless Bevy runtime check compared 1 and 64 buildings over 200 updates after
10 warm-up updates using the default hut palette. Both runs retained exactly **14 meshes, 10 materials, and
9 images**. Sixty-four placements preserved independent door states, transitioned
through all three LODs, and returned to the original entity count after cleanup.

The latest local run averaged 91.57 microseconds per update for 1 building and
173.85 microseconds for 64. These are CPU ECS updates without a renderer or GPU;
they are observations from this machine, not performance guarantees. Recorded
component payload counts exclude allocator capacity, heap allocations in strings
and collections, source geometry, resources, and GPU memory.

Prototypes own generated data. Instances share compiled assets through `Arc` and
Bevy handles. Recipe clones are intentional for history, worker snapshots, and
the independent generated-prototype boundary; borrowing the mutable document
cannot supply those lifetimes. Bevy mesh uploads own their vertex/index buffers.
The exporter borrows the large generated buffers while serializing.

## Review artifacts

Local artifacts are under `artifacts/building-studio/2026-11-10/` at the repository
root. This directory is Git-ignored, matching the repository's artifact policy.
It contains rendered PNGs, `runtime-metrics.json`, the Planet smoke output,
`implementation.patch`, and `changed-files.txt`. Portable package generation was
also verified under `/tmp/building-studio-final-20261110/hut-package/`.

The reusable source and three example recipes are under `island-building/`.
The packaged app is `island-building/dist/Building Studio.app`; `dist/` and
build output are ignored. Recreate the debug bundle with:

```sh
./island-building/package-macos.sh --debug
```

Initial grammar: single-storey rectangles, gable/single-slope roofs, parallel
room partitions with open internal passages, hinged exterior doors, and fixed
transparent windows. Multi-storey stairs, joined wings, furniture, settlements,
and Unity import are separate extensions.

## Door and file-picker follow-up

The door leaf now has 3 mm side/head clearances and 6 mm floor clearance, with a
continuous jamb/head lining through the wall. Corrected reveal winding keeps the
lining visible from the opening. The inward swing is retained. Geometry identity
and exported geometry metadata advanced to version 2 in that follow-up so old
prototypes could not be reused under the new geometry hash.

The texture selector failure was reproduced as a deadlock: synchronous native
file selection dispatched to AppKit while the main thread waited for the Bevy UI
system. Open, Save As, package destination, and both material pickers now use
asynchronous native dialogs outside the ECS callback. Completion is bound to the
authoring generation and recipe revision. Cancellation, failed saves, undo, and
save continuations retain their document safeguards.

Live checks opened both material pickers, selected CrackedStone for the wall,
observed the changed preview, cancelled the roof picker, and saved the recipe
through Save As. The final signed app was reopened; the fitted door was inspected
closed and open, and the material picker opened and cancelled successfully.
All 27 library, 12 core integration, and 194 Planet tests passed after the fixes;
strict new-crate Clippy with all targets passed. The rebuilt app is left open.
A focused follow-up diff and closed-door capture are under
`artifacts/building-studio/2026-11-10/door-dialog-fix/`.

## Door material, knobs, and wooden planks

Door leaves accept an optional material recipe, including physical tiling,
normal/AO maps, LOD1 proxy surfaces and the LOD2 exterior bake. Legacy recipes
without `door_recipe` still load with the palette colour. Save As rebases this
dependency; portable export captures and verifies its JSON and maps. Reselecting
a material explicitly refreshes a dependency edited in Material Studio.

Each door has a round knob with a rose and stem on both faces, attached to the
hinged leaf. Hardware uses a separate smooth metal material, with no inherited
wood texture. The shared prototype owns one hardware mesh per door; placements
reuse its handles. Hardware follows leaf rotation and LOD visibility and remains
outside structural collision. Exported hinge-local hardware OBJ files accompany
the leaves. Current geometry/material/bake identities are 3/2/2.

The canonical `island-rs/texture-recipes/wooden-planks.json` uses the shared Rust
WoodenPlanks evaluator: six continuous vertical boards over a 1.2 × 2.4 m tile,
recessed seams, bevels, longitudinal grain, board variation, a colour parameter,
and optional staggered end joints. Material Studio exposes these controls.
Both macOS app bundles were rebuilt and ad-hoc-signature verification passed.
The previous bundles remain under their ignored `dist/.previous/` directories.

Verification: 30 Building library tests, 13 Building core tests, 194 Planet tests,
10 material editor integration tests, and two wood sampler tests passed. Strict
Clippy passed for Building with all targets and for the shared renderer-free
native library. Material Studio all-target checks passed; its separate Bevy test
suite was not run. Existing material golden hashes remained unchanged.

Metal captures show the wood door and knob with the leaf closed and open. Live
Building Studio review applied the bundled wooden planks using the new shortcut,
opened/cancelled the door JSON picker, and operated the textured door. The app
is left open on the closed wooden entrance. The previously open recipe was saved
to `/tmp/building-before-wooden-door-20261110.json` before the update. Portable
export regenerated with the same prototype hash and retained its material and
hardware assets. Artifacts and the focused diff are in
`artifacts/building-studio/2026-11-10/door-material-wood/`.

The independent workspaces share a native crate that emits an unhashed
`libmotu.rlib` alongside its CDylib. A different workspace can replace that
artifact while Cargo still considers the dependency fresh, causing incompatible
Serde-trait identities. The final builds were serialized and the native source
timestamp refreshed before each workspace rebuild. These controlled checks all
passed without dependency version changes.


## Chimneys and fireplaces

The previous reviewed Building Studio implementation was committed and pushed as
`04924a4fbb3eb0f663ec209ad10079ba3b2a8049` before this extension. The chimney
changes are uncommitted on `main`; unrelated Unity weather changes remain intact.

New presets include one optional wall-attached fireplace. Legacy JSON without
`chimney` loads with no chimney and retains its 4 × 2 atlas layout. LOD0 has a
recessed firebox, soot lining, jambs, lintel, raised hearth, tapered hood, hollow
flue and open chimney cap. Roof, ceiling, ribs and ridge fascia are clipped around
the shaft, including ceiling collision. Placement validation protects exterior
openings, full door sweeps, interior doorway approaches and room routes. The
hearth is a static obstacle; it does not become a raised walking surface.
LOD1 keeps an exterior shaft and rim; LOD2 uses a minimal stack and cap with
baked masonry charts in a 4 × 4 atlas. The atlas mirrors tangent-space normal
green to match its reversed V direction. Current geometry/material/bake versions
are **4/3/3**.

Chimney and fireplace masonry share an optional material recipe. The embedded
`coursed-stone.json` default uses the shared Masonry evaluator, staggered courses,
recessed mortar and a stone-colour parameter. The default palette recolours stone
while preserving relief. A custom recipe retains its own authored colours and
physical tile dimensions. Save As rebases the chimney path; portable export
snapshots and verifies the dependency. The soot lining has a separate dark,
rough PBR material. Fire, smoke and emissive lighting are outside this extension.
Recipe snapshots and generated prototypes retain their existing owned boundaries;
geometry helpers borrow recipe data and instances continue sharing compiled assets.

Final checks passed: **39 Building library tests, 18 core tests, 194 Planet tests,
and 11 shared material-editor integration tests**. Strict Building Clippy with
all targets, shared renderer-free native-library Clippy, formatting and focused
Git whitespace checks passed. New checks cover real firebox depth, open flue
through roof and ceiling, all facades and both roof families, cap/roof bounds,
validation conflicts, legacy JSON, normal parity, material invalidation,
Save As/reopen, portable regeneration and hearth collision across visual LODs.
The repeated-instance ownership/cleanup test also passed; the earlier CPU timing
and asset counts above describe the pre-chimney baseline.

Final Metal captures were inspected for the hut fireplace and all three exterior
LODs, the partitioned cottage fireplace and the single-slope workshop chimney.
An exported hut with custom chimney/wood-door dependencies passed all **28 file
hash checks**; the automated export test deleted the source dependency directory
and regenerated the same prototype hash and atlas from the portable package.

The version-4 macOS debug bundle was rebuilt and its ad-hoc signature verified.
Its executable code-section hash matches the verified target executable (whole
binary hashes differ because packaging signs the binary). The bundled stone JSON
matches the canonical source bytes. Live UI review assigned RoundedRiverStones
through the chimney JSON picker, observed the changed finish, then used Clear to
restore Coursed stone. Holding W twice stopped at the hearth; selecting LOD2 while
inside retained LOD0. The app is left open in Auto on the Chimney tab and Fireplace
view. The prior wooden-door draft was preserved at
`/tmp/building-before-chimney-20261110.json`, and the previous bundle is under the
ignored `dist/.previous/` directory.

Logs, final PNGs, the portable package, package/hash checks, changed-file manifest
and focused patch are under
`artifacts/building-studio/2026-11-10/chimney-fireplace/`.


## Straight fireplace breast and wall-coloured mantel

The LOD0 stone breast now retains its full width and depth up to the ceiling,
with the tapering flue transition concealed inside. Raised surround jambs, a
header and a projecting mantel shelf reuse `MaterialSlot::Interior`, the exact
material used by the adjacent room walls. Changing the interior palette colour
therefore changes the wall, surround and mantel together. The stone breast,
hearth and exterior stack retain the assigned chimney material.

New collision fills the straight breast around the open flue and covers the
surround and shelf. All trim stays within the existing hearth footprint. No
recipe schema change or material copies were introduced: geometry helpers borrow
the recipe and reuse the existing material slot. The exterior LOD1/LOD2 meshes
are unchanged. Geometry identity/export version is now 5; material and bake
versions remain 3. The macOS bundle advances to version 5.

Verification: **39 library tests and 19 core tests passed**, including ray checks
on every facade for constant breast width up to the ceiling, matching collision,
wall-material surround/header/shelf and an unobstructed firebox. Existing open-flue,
roof, LOD, portable-export and runtime collision tests also passed. Strict Clippy
with all targets, formatting and focused whitespace checks passed. The actual
Metal fireplace render was inspected. Logs, PNG and the incremental diff for this
follow-up are under `artifacts/building-studio/2026-11-10/straight-fireplace/`.
The open draft was preserved before updating the app at
`/tmp/building-before-straight-fireplace-20261110.json`. Changes remain uncommitted.

The signed version-5 app was launched and inspected in the Fireplace view. The
straight stone breast, projecting mantel and wall-coloured surround were visible;
the app is left open on the Chimney tab for review.

## Slate and thatch roof coverings

The Roof tab now selects Metal, Slate or Thatch independently of the gable or
single-slope shape. Legacy recipes default to standing-seam metal. Selecting a
covering restores its default roof colour and material; custom roof JSON remains
available through Materials. Thatch retains an effective thickness of at least
0.28 m without moving the underside. Existing roof dimensions are preserved.

LOD0 adds physically spaced slate overlap courses or thatch bundles and eave
detail. LOD1 removes those details while keeping the covering silhouette, and
LOD2 uses the baked exterior shell. Thatch retains its raised ridge at every LOD.
Detail counts are bounded for large roofs. Geometry is clipped around the real
chimney opening, preserving the hollow flue for both shapes and all coverings.

The shared native material evaluator and Material Studio support editable
SlateRoof and ThatchRoof models, with reusable `slate-roof.json` and
`thatch-roof.json` recipes. The built-in materials bind `roof_colour` to the
building palette; custom JSON retains its authored colour. Texture U follows the
slope from eave to ridge and V follows the ridge. Mirrored right-gable atlas
sampling also flips the tangent normal's red channel; the existing atlas green
conversion is retained.

New roof sources retain at least 256 px in editing previews to resolve their
fine detail. Slate/thatch atlas sampling uses temporary roof-only mip levels,
linear-light colour averaging, normalized normals and bilinear/trilinear
filtering. Other source maps remain borrowed and the legacy metal bake is
unchanged. Geometry/material/bake identity versions are now **6/4/5**. The macOS
Building Studio bundle advances to version 6. An inspector gesture fix also
keeps Undo/Redo enabled when pressing their toolbar buttons.

Final verification passed: **43 Building library tests and 22 core tests**,
including roof/chimney bounds, detail limits, legacy defaults, UV and normal
parity, palette/material invalidation, filtering, and portable regeneration.
Shared checks passed **3 roof sampler tests, 13 material-editor integration
tests, 47 Material Studio tests and 194 Planet tests**. Strict Building all-target
Clippy, renderer-free native-library Clippy, formatting and focused whitespace
checks passed. Existing material golden hashes remain unchanged.

Actual Metal renders were inspected for both coverings at all three LODs. This
caught coarse-atlas slate aliasing, corrected by the roof filter and rechecked in
`slate-lod2-final.png` and `thatch-lod2-final.png`. Final portable packages passed
all **23 file hash checks each**. A custom-slate dependency test also deletes its
source directory and regenerates the same prototype hash and atlas from the
export. Both studio app bundles were rebuilt and their ad-hoc signatures
verified; Material Studio includes the canonical roof recipes.

Live Building Studio review switched between Slate and Thatch while retaining
the user's 4 m roof rise, then verified that Undo restores Thatch. The original
draft is preserved at `/tmp/building-before-roof-coverings-20261110.json`; the
review draft is `/tmp/building-roof-coverings-review-20261110.json`. The app is
left on Roof with Thatch and Auto selected. Changes remain uncommitted on `main`;
unrelated weather changes are preserved. Logs, captures, final portable packages,
the changed-file manifest and incremental patch are under
`artifacts/building-studio/2026-11-10/roof-coverings/`.

## Thatch realism follow-up

Reference review used TC Thatching's real cottage, Dartmoor Thatch's ridge
close-up and Kobayashi's cut-edge photograph. The cottage and ridge references
showed a continuous dense coat, deep rounded edges, broad ridge dressing and
localized hazel fixings. Reference URLs and observations are saved in
`artifacts/building-studio/2026-11-11/thatch-realism/references.json`; downloaded
photographs are review references only, not shipped material assets.

Thatch now has its own continuous tessellated coat with smooth analytical
normals, gentle seed-driven undulation, rounded eave/verge returns and a broad
curved ridge. Its minimum effective depth is 0.45 m; the structural underside and
authored rise remain fixed. LOD0 adds scattered tapered raised reed tips over
the coat and edges, plus thin hazel rails and crossing spars confined to the
ridge. LOD1 and LOD2 use coarse versions of the same rounded profile. Slate and
metal retain their previous geometry. All coat, return, reed and binding faces
are clipped around the complete chimney shaft. Flashing follows the curved
surface; simplified masonry extends below the lowest rounded return.

The material replaces the repeating bundle/course grid with independently
jittered reed profiles, staggered short tips, subtle packing variation and
locally broken gold/grey-brown pigments. Wrapped segment indices preserve
tiling. Colour rebinding still preserves height, normals and AO. Building Studio
keeps the 1024 px roof source even with its 128 px editing atlas; other materials
retain their previous budgets. The new default pigment is muted natural straw;
saved recipes retain authored palette values. Selecting another covering then
Thatch restores the new default. Geometry/material/bake versions are **7/5/6**;
the Building Studio bundle is version 7.

Final checks passed: **43 Building library tests and 22 core tests**, **6 roof
sampler tests**, **13 material-editor integration tests**, and **47 Material
Studio tests**. Strict Building all-target and renderer-free native-library
Clippy, formatting and focused whitespace checks passed. The 194 Planet tests
also passed before the final reed-detail/pigment polish. New checks cover rounded
eaves and ridge at every LOD, physical UVs, repeatable seed variation, mesh winding,
full bounds, chimney clearance and explicit LOD triangle budgets. A detected
edge-fringe bounds escape was fixed by clamping centres to each reed's width.

Final serial Metal captures passed for the user's 4 m roof at LOD0/1/2, a cottage
and a single-slope workshop. Initial simultaneous captures wrote images but two
timed out during GPU shutdown; the serial final runs all exited successfully.
The 4 m draft's exported static LOD triangle counts are **52,726 / 3,586 / 1,564**,
reflecting the intentionally increased near detail. Its package passed all
**23 file hash checks**. Both studio bundles were rebuilt and their ad-hoc
signatures verified, with prior bundles preserved under `dist/.previous/`.

Live app control was unavailable: the native pipe and browser host could not
start. The interactive app was therefore not restarted or edited, and opening
the review diff in the current editor could not be verified. Final captures use
the actual Metal renderer through the studio's capture interface. The focused
patch and changed-file manifest are saved alongside the captures and logs under
`artifacts/building-studio/2026-11-11/thatch-realism/`. Changes remain uncommitted
on `main`; unrelated ocean and sailing work is preserved.

## Broad thatch saddle ridge

The ridge now forms a thick saddle extending about 1.1 m down each slope, with
a low rounded crown and rolled lower hems. Physical slope length determines
width, so the user's 4 m rise no longer collapses it into a narrow ridge roll.
The coat, all three LOD meshes and hazel bindings borrow the same profile;
additional fixed stations retain the crown and hems in the distant mesh.
Relief stays within the existing 0.22 m allowance and chimney clipping is retained.
Geometry identity/export version is now 8; material/bake versions remain 5/6.
The signed macOS bundle advances to version 8.

Verification passed: **43 library tests and 23 core tests**, strict all-target
Clippy, formatting and focused whitespace checks. The new ray checks confirm
saddle depth 85 cm down both slopes, the return to the main coat beyond each
hem, and bounded crown height at rises 1.4/4/6 m in every LOD. Existing mesh
winding, UV, bounds, triangle-budget, chimney and portable-export checks pass.
Metal captures of all three 4 m roof LODs and the cottage were inspected.

Live review loaded the rebuilt app and confirmed the broad saddle on the 4 m
roof. The original open draft is saved at
`/tmp/building-before-thatch-saddle-20261111.json`; the app is left open on a
separate thatch review recipe in Auto. The previous bundle is preserved under
`dist/.previous/`. Logs, captures and the focused diff are under
`artifacts/building-studio/2026-11-11/thatch-saddle/`. Changes remain uncommitted.

## Wrapped saddle ends and four straw shells

The saddle now drapes over both gable apexes at every LOD. Its lower apron joins
the existing underside, closing a daylight gap discovered in the first Metal
close-ups. The original structural roof height and authored rise are retained.
Shared cut-face sampling and chimney clipping keep the return, apron and straw
detail aligned. Bounds include the additional 24 mm end projection; the same
checks also exposed and fixed a pre-existing 45 mm frame-trim bounds escape on
roofs without overhang.

LOD0 adds exactly four masked cut-end shells at 5/10/15/20 mm offsets. They cover
the exposed rounded ends and edges rather than duplicating the roof surface.
Four seeded 256 px maps share reed centres over a 16 cm physical tile, with
nested, progressively sparser tip coverage and pigment taken from the assigned
roof albedo. Bevy's standard PBR mask path uses cutoff 0.5 and double-sided
surfaces in colour, depth and shadow passes. This adds four shared-material
draws in LOD0; LOD1 and LOD2 have no shells. Slate and metal retain their existing
geometry. No custom shader or native plugin change is required.

Portable exports include opacity PNGs, MTL `map_d` references and four explicit
masked-material entries in the manifest. Exported UVs now include the physical
material scale used by the renderer. Geometry/material/bake versions are
**9/6/6**; the rebuilt, signature-verified macOS bundle is version **9**.

Final verification passed **46 Building library tests and 23 core tests**,
strict all-target Clippy, formatting and focused whitespace checks. Coverage
includes end coverage and underside closure at three roof rises in every LOD,
shell count/offsets/UVs/alpha nesting, real chimney intersections, bounds,
winding, material compilation and portable regeneration. The **194 Planet
tests** passed after shell integration and before the final underside closure.

Final serial Metal renders were inspected in `roof-end-final.png`,
`cottage-end-final.png` and `exterior-final-lod0.png` through
`exterior-final-lod2.png`; all capture processes exited successfully. The 4 m
roof's final exported static triangle counts are **61,956 / 4,422 / 2,148**,
excluding the moving door and hardware. All **39 exported file hashes and byte
lengths** passed verification in `package-checks-final.json`.

The user's open draft was saved before replacing the app at
`artifacts/building-studio/2026-11-11/thatch-saddle/building-before-thatch-ends-20261111.json`.
Live review loaded its separate `thatch-ends/open-draft.json` copy into version
9, verified the wrapped apex and straw detail, and left the app on Roof with
the new **Roof end** inspection view, Auto LOD and the entrance open. The
original 4 m rise, 0.50 m thickness and 128 px editing atlas are retained.
Captures and the final portable package use a 512 px atlas. The previous bundle
is preserved under `dist/.previous/`. Logs, final captures, the export,
changed-file manifest and incremental patch are under
`artifacts/building-studio/2026-11-11/thatch-ends/`. Changes remain uncommitted on
`main`; unrelated ocean and sailing work is preserved.

## Filled triangular saddle ends and Cargo editor validation

The gable saddle ends now have a horizontal lower hem joining both slopes,
forming a filled triangle across the apex. The front face, flat underside and
rear closure share the existing borrowed roof profile. All three LODs retain
the filled shape and chimney clipping; the four existing LOD0 straw shells
follow the reshaped cut face. The Roof end camera now frames the complete cap
and its lower hem. Geometry/material/bake versions are **10/6/6**, and the signed
macOS bundle is version **10**.

Verification passed **46 Building library tests and 23 core tests**, strict
all-target Clippy, formatting and focused whitespace checks. New ray probes
verify solid coverage below the structural apex, the horizontal hem and the
closed underside at both ends, three roof rises and all three LODs. Existing
shell, winding, bounds, chimney and portable-regeneration checks still pass.
Final close-ups `roof-end-final-lod0.png` through `roof-end-final-lod2.png`, plus
`exterior-lod0.png` and `rear-lod0.png`, were inspected using actual Metal
renders. All capture processes exited successfully. The package passes all
**39 file hash and length checks**, with static triangle counts of
**62,120 / 4,530 / 2,248** excluding the moving door and hardware.

Live review reopened a separate copy of the saved draft with its authored
4 m rise and 0.50 m thickness, then verified the filled cap in Roof end view.
The app is left on Roof, Auto LOD and an open entrance. The prior version 9
bundle remains under `dist/.previous/`.

VS Code's Problems panel reported two Even Better TOML schema errors on the
valid `[lints.rust]` and `[lints.clippy]` tables. Workspace settings now associate
Cargo manifests with SchemaStore's self-contained Cargo schema at commit
`73fd8db58e81db8e31be43cb70dbff3ea140890d`, which supports edition 2024 and
avoids the failing nested lint-schema references. Manifest validation remains
enabled and no Cargo lint settings changed. Live verification confirmed
**No problems have been detected in the workspace** after the association took
effect. The schema pin is workspace-local; other editor settings are retained.

Logs, source snapshot, final captures, portable package, draft copy, schema
evidence and focused diff are under
`artifacts/building-studio/2026-11-11/thatch-triangle/`. Changes remain
uncommitted on `main`; unrelated ocean and sailing changes are preserved.

## Blended cut-straw ends and exposed undersides

The filled triangular saddle cap now has a rolled lower lip and wider, smooth
shoulder transitions into the verges. Its opaque `ThatchEnd` material covers the
front face, rounded outer edge, lower lip, rear closure and exposed overhang
undersides. Interior roof surfaces inside the building retain their interior
finish. Physical cut-face UVs continue around the lip; the front face no longer
collapses to one row of the roof texture. The new 256 px end-grain maps use
jittered reed sections, small hollow centres, normal relief and roughness, with
pigment derived from the assigned roof material.

Exactly four LOD0 straw shells now offset the same faces as the opaque cap and
continue underneath the lip. This removes patches where the old coarse shell
chords sank into the curved cap. The rounded upper band uses opaque end grain;
the four fur layers stay on the cut faces and lower lip. LOD1 retains the end
material without shells. LOD2 bakes filtered end grain into the existing spare
atlas chart 7, retaining the single atlas draw. Slate and metal keep their
existing material and geometry paths. Geometry/material/bake versions are
**11/7/7**, and the rebuilt signed macOS bundle is version **11**.

Final checks passed **47 Building library tests and 23 core tests**, strict
all-target Clippy, formatting and focused whitespace checks. Regression checks
cover opaque end material compilation/export, normal and pigment variation,
four outward shell layers over the curved face and beneath the lip, straw
material on the exposed underside, and the distant end-grain atlas chart.
Existing bounds, winding, flue clipping, repeatability and **250k/15k/4k**
triangle-budget checks pass at the 40 m size limit.

Seven serial Metal captures were inspected: `blended-end-lod0.png` through
`blended-end-lod2.png`, `blended-under-lod0.png` through
`blended-under-lod2.png`, and `exterior-final-lod0.png`. Every capture process
exited successfully. All **42 portable file hash and byte-length checks** pass;
the four masked material entries remain intact alongside the new opaque end
maps. The final 4 m draft's static triangle counts are
**81,532 / 4,558 / 2,268**, excluding its moving door and hardware.

Live review loaded `thatch-blend/open-draft.json`, a separate copy of the saved
draft, into version 11 and inspected the finish using the new **Roof under**
view. The app is left on Roof, Roof under, Auto LOD and an open entrance, with
the authored 4 m rise and 0.50 m thickness retained. The prior version 10 bundle
is preserved under `dist/.previous/`. Logs, source snapshot, final captures,
portable package, draft copy and focused review files are under
`artifacts/building-studio/2026-11-11/thatch-blend/`. Changes remain uncommitted
on `main`; unrelated work is preserved.


## Solid thatch ridge and level eave caps

Thatch now uses closed convex cross-sections extruded along the roof: a separate
solid ridge prism with a horizontal underside, straight run prisms and horizontal
cuts at the low eaves. The ridge keeps its physical 1.1 m reach and has a distinct
12 cm shoulder above the main coat. Blended joins, rounded return rings and the
rolled lip are removed. Minimal wall-line wedges close the underside where a
horizontal cut rises above the structural wall. Hazel bindings use single
straight segments on each planar ridge face.

Packed straw end grain and four masked 5/10/15/20 mm shells remain on exposed
cut faces at LOD0, including the flat ridge bottom and eave caps. LOD1 retains the
same closed silhouette without shells; LOD2 uses the baked atlas. Physical roof
UVs, exact shaft-footprint clipping and flue clearance remain covered. Geometry,
material and bake identity/export versions are **12/7/8**. The signed macOS bundle
advances to version **12**. A new **Eaves** inspection view frames the full gable
from standing eye height; Roof end and Roof under retain their close views.

The 47 library tests and 23 core tests pass with studio enabled. Strict all-target
Clippy also passes. Geometry checks cover gable/shed roofs at 1.4, 4 and 6 m rises,
solid ridge ends and bottoms, sharp shoulders, horizontal eaves, shell offsets,
mesh winding, authored bounds, physical UVs and real chimney/shell intersections.
The current recipe's static LOD triangle counts are **27,317 / 344 / 134**, down
from **81,532 / 4,558 / 2,268** (66.50% / 92.45% / 94.09% fewer). Door meshes are
excluded consistently from both sets. All 42 portable package files match their
manifest lengths and SHA-256 hashes, and all four masked materials are retained.

Serial Metal captures cover the ridge end, underside and exterior at all three
LODs. The distant exterior retains the distinct ridge profile and baked roof
colour/detail. Evidence, the saved draft copy, export checks and focused source
diff are under `artifacts/building-studio/2026-11-11/thatch-prism/`. Changes remain
uncommitted on `main`; no commit or push was requested for this follow-up.

Live review opened the signed version-12 canonical app and loaded the preserved
`thatch-prism/open-draft.json`. It is left on Roof, Eaves and Auto, with the
entrance open and the authored 4 m rise / 0.50 m thickness retained. The full
ridge and both level lower caps are visible. Version 11 is preserved under
`island-building/dist/.previous/pre-prism-thatch-20261111-133422/`.


## Projecting thatch ridge ends

The solid ridge cap now projects **12 cm beyond the main roof at both gable
ends**, retaining its closed triangular faces, flat straw underside and four
LOD0 masked shells. The same extension remains at LOD1 and baked LOD2. Render
bounds include the projection, and source/atlas UV extents include its full
length so the end surfaces stay inside their padded charts. No geometry loops
or additional triangles were introduced: the saved recipe still has **27,317 /
344 / 134** static triangles. Geometry/material/bake versions are **13/7/9**;
the signed macOS bundle advances to **13**.

All **47 library and 23 core tests** pass with studio enabled, including 12 cm
end-face positions at every LOD, shell offsets, bounds, physical UVs, chimney
clearance and portable export regeneration. Strict all-target Clippy and format
checks pass. Five serial Metal captures cover the close end at all three LODs,
LOD0 underside and full Eaves view; every capture exits successfully. Evidence
and the preserved saved-draft copy are in
`artifacts/building-studio/2026-11-11/ridge-projection/`. Changes remain
uncommitted on `main`.

Live review loaded the preserved recipe into the signed version-13 canonical
app. It is left on Roof, Roof under and Auto, with the entrance open, showing the
projecting cap and straw underside. The prior version-12 app is preserved at
`island-building/dist/.previous/pre-ridge-projection-20261111-143844/Building Studio.app`.


## One smoothing subdivision for LOD0 thatch

LOD0 now applies exactly **one Catmull-Clark subdivision** to each closed roof
run and the separate ridge cap. Short supporting edges localize the rounding;
after subdivision, each axis is refitted to the control piece's original minimum
and maximum. Smooth lighting normals are calculated after refitting. Material
seams and the chimney shaft are cut afterward, preserving shared geometry and
the full flue opening. LOD1 and LOD2 retain their previous geometry and atlas.

Cut-straw material wraps the rounded end bevels. The four 5/10/15/20 mm shells
extend along the cut reeds independently of smooth lighting normals, avoiding
folded triangles on thin edges. Reeds, segmented hazel bindings and near chimney
flashing sample the new surface. A concealed 10 cm wall overlap closes the
wall-to-roof joint beneath the rounded underside. Original collision, layout
bounds and door descriptors are retained. Generator/material/bake versions are
**14/7/9**, and the signed macOS app advances to version **14**.

All **49 library and 23 core tests** pass, as do strict all-target Clippy and
format checks. New tests check the single iteration (six cube faces become 24
quads), exact bounds restoration, closed edges, deterministic output and smooth
surface sampling. Existing coverage checks the roof families, steep/shallow
profiles, 40 m size limit, winding, shell offsets, physical UVs and chimney holes.
The current recipe uses **34,155 / 344 / 134** static triangles, compared with
**27,317 / 344 / 134** before smoothing.

The final portable package's 42 files match their manifest lengths and SHA-256
hashes. Compared with the pre-change export, `lod1.obj`, `lod2.obj`, all three
atlas maps, `collision.json`, `layout.json` and `doors.json` are byte-for-byte
identical. Serial Metal captures include close ends, undersides, whole exteriors
and standing-height eaves. Final LOD0 captures show the end-grain finish over
the rounded edges and a closed gable-wall joint. Evidence and focused source
review are under `artifacts/building-studio/2026-11-11/thatch-smooth/`. Changes
remain uncommitted on `main`.

Live review opened the signed version-14 canonical app and the preserved
`thatch-smooth/open-draft.json`. It is left on Roof, Roof end and Auto, with the
entrance open and the authored 4 m rise / 0.50 m thickness retained. The rounded
cap and roof ends are visible with the cut-straw finish. Version 13 is preserved
at `island-building/dist/.previous/pre-thatch-smooth-20261111-152251/Building Studio.app`.


## Inset LOD0 windows with casings and sills

Near-LOD windows now have painted outer architraves, full-depth jamb/head/bottom
linings, recessed sashes and glazing bars fitted around the existing clear-glass
plane. A shallow sloped sill projects **12 cm outside the wall**, has 7 cm horns
beyond the opening and extends 2.5 cm into the room. All pieces use the existing
Trim material and palette. Dedicated casing solids replace the old flat frame
and reveal planes. Render bounds include the complete casing and sill even
with zero roof overhang; glazing and physical collision planes are retained.

The new **Inspection → Window** view frames the first window obliquely and is
also available through `--view window`. Geometry/material/bake versions are
**15/7/9**, and the signed macOS bundle advances to **15**. All **50 library and
24 core tests** pass, including window camera framing, recession, pane clearance,
sill slope/projection, winding and bounds on all four facades at 0.08/0.20/0.60 m
wall thickness. Strict all-target Clippy and formatting checks pass.

The saved recipe has **34,355 / 344 / 134** static triangles, adding 100 triangles
per LOD0 window. All 42 portable package files match manifest lengths and SHA-256
hashes. Against the version-14 package, both lower-LOD OBJ files, all three atlas
maps, collision, doors and this recipe's layout are byte-for-byte identical.
Serial Metal captures cover the close window, room interior and exterior at all
three LODs. The additional `final-window-lit-lod0.png` uses ambient 1200 / exposure 9.2
to show the shaded casing more clearly; the app retains its normal lighting.
Evidence and focused source review are under
`artifacts/building-studio/2026-11-11/inset-windows/`. Changes remain uncommitted
on `main`.

The visible pane is inset 4 cm beneath the sash beads, keeping glass clear of
the sloped sill while preserving the full physical collision aperture and the
original glazing depth. The final test run and captures include this fit.

Live review opened the signed version-15 canonical app with the preserved
`inset-windows/open-draft.json`. It is left on Openings, the Window inspection
view and forced LOD0, with the entrance open and the east-window controls
expanded. The casing, recessed glazing bars and projecting sill are visible.
The previous version-14 app is preserved at
`island-building/dist/.previous/pre-inset-windows-20261111-154812/Building Studio.app`.

## LOD0 sill-to-frame fit

Raised the sloped sill's upper face by 25 mm while retaining its lower face and
12 cm projection. Its low edge now sits 10 mm above the opening cut, closing the
visible strip of exterior wall beneath the frame. Removed the separate flat
bottom lining: the sill now forms this surface itself, avoiding competing faces
at the join. Clear panes, glazing depth and collision apertures are retained.

All 50 library and 24 core tests pass. The existing window regression now casts
a ray just above the lower opening cut and verifies that the sill closes it on
all four facades at 0.08/0.20/0.60 m wall thickness. Strict all-target Clippy and
format checks pass. The saved recipe has 34,331 / 344 / 134 static triangles,
removing 12 triangles per near-LOD window. Both lower-LOD meshes, the three atlas
maps, collision, doors and layout match the version-15 package byte for byte;
all 42 exported files match manifest lengths and SHA-256 hashes. Generator,
material and bake versions are 16/7/9.

Metal captures cover the shaded window, sunlit window and interior. The sunlit
review recipe only moves the west window first in the opening list so the
Window camera targets it; authored dimensions and normal lighting are retained.
Evidence and focused source review are under
`artifacts/building-studio/2026-11-11/window-sill-fit/`.

The signed version-16 canonical app was reopened with the original preserved
`inset-windows/open-draft.json`, reviewed at close range and left on Openings /
Window / forced LOD0, with the entrance open and east-window controls expanded.
Version 15 is preserved at
`island-building/dist/.previous/pre-window-sill-fit-20261111-161018/Building Studio.app`.
Changes remain uncommitted on `main`.

## Wattle-and-daub walls

The preceding chimney, roof and window work was committed and pushed to `main`
as `ff218e5` before this extension. `walls.style` now offers `plain` (the legacy
default) and `wattle_and_daub`. Defaults are 14 cm timber width, 25 mm outward
projection and 1.5 m bays. Structure exposes these dimensions; Materials exposes
an independent timber colour and optional material JSON. The default wood grain
follows each member. A reusable `wattle-daub.json` plaster material and complete
`wattle-daub-cottage.json` building recipe are included and packaged.

One clipped polygon layout supplies posts, rails, braces and gable/shed framing.
It excludes opening casings and sills, partitions timber joints without competing
front faces and stays inside the wall profile. LOD0 adds the projecting edges,
LOD1 retains the same fronts without those edge faces, and LOD2 bakes the layout
into the unchanged minimal shell. Baked coverage filters diagonal edges in linear
light, rotates wood normals with the grain and carries the projected relief.
Interior surfaces, doors and structural/window collision remain as before.

All **52 library and 26 core tests** pass with studio and all targets enabled;
strict all-target Clippy and formatting checks pass. Coverage includes legacy
defaults, dimension validation, profiles at 3 m and 40 m scales, frame joints,
opening clearance, actual projection on every facade, both roof shapes, valid
winding/bounds, deterministic generation, independent timber recolouring and
portable timber dependencies after Save As and export. Versions are **17/8/10**
for geometry/material/bake, and the macOS bundle is **17**.

The preserved 4 m-rise thatched draft has **37,080 / 923 / 134** static triangles.
Metal captures show the entrance, window and exterior at all three LODs; the
final LOD2 capture includes the filtered frame edges. The new cottage recipe
also passes CLI validation. Evidence and focused source review are under
`artifacts/building-studio/2026-11-11/wattle-daub/`. This extension is uncommitted.

Before rebuilding, the open untitled version-16 recipe was saved to
`wattle-daub/pre-rebuild-draft.json`. The old app became unresponsive after Save
As; the complete JSON backup was verified before closing it with macOS Force
Quit. The saved recipe is retained separately from the review draft.

All 45 files in the final portable package match manifest lengths and SHA-256
hashes. The signed version-17 canonical app was opened with `wattle-draft.json`.
Live switching to Plain removed the frame; Undo restored the wattle-and-daub
surface, frame controls and clean saved-document state. The app is left on
Structure / Overview / forced LOD0, with the entrance open. The previous version
16 bundle is preserved at
`island-building/dist/.previous/pre-wattle-daub-20261111-165145/Building Studio.app`.

## Continuous timber corners and covered roof joints

Timber faces now meet on a common miter edge outside each building corner.
Their UV origins share the same wood-grain phase and height, and opening-jamb
station merging preserves the corner posts. The thatch's former 10 cm plaster
overlap is replaced by the shared timber roof band. Its top follows the roof
profile and tucks into the coat, hiding the pale strip without moving the roof
or changing the authored rise, covering thickness or smoothing. Shed framing
also follows the structural underside rather than the top of the roof slab.

All **53 library and 27 core tests** pass with studio and all targets enabled;
strict Clippy and formatting checks pass. New regressions cover shared corner
positions and grain phase, nearby opening jambs, actual corner closure and
roof-band coverage on all facades with both roof shapes and 5/40 mm projections.
The new **Inspection → Corner** view is also available as `--view corner`.

Serial Metal captures show the close corner at every LOD and the full LOD0
gable-to-thatch join. All 45 package files match manifest lengths and SHA-256
hashes. Against version 17, every LOD0 roof/thatch part, all material maps,
collision, doors, layout and the LOD2 geometry are unchanged. The saved draft
uses **37,269 / 958 / 134** static triangles. Geometry/material/bake versions are
**18/8/11**. Evidence and focused source review are under
`artifacts/building-studio/2026-11-11/timber-joints/`.

The signed version-18 canonical app was opened with the original
`wattle-daub/wattle-draft.json`. It is left on Structure / Corner / forced LOD0,
with the entrance open and no unsaved recipe changes. The shared post and
covered roof edge are visible. Version 17 is preserved at
`island-building/dist/.previous/pre-timber-joints-20261111-174944/Building Studio.app`.
These changes and the preceding wattle-and-daub extension remain uncommitted.

## Opening crosspieces

The preceding wattle-and-daub and joined-corner work was committed and pushed
to `main` as `47549c2` before this extension. The shared frame layout now adds
horizontal members directly above and below windows and above doors. Each spans
between the upright posts, uses the configured timber width/projection/material
and has continuous horizontal grain. Crosspieces take precedence at their joints
so vertical members butt into them. Their inner edges fit the actual casing and
sill bounds; the transparent panes and door apertures remain unobstructed.

All **54 library and 28 core tests** pass with studio and all targets enabled.
Strict all-target Clippy and formatting checks pass. New coverage verifies
horizontal grain, fit at the trim edges and clear apertures on every facade
with both roof shapes. Serial Metal captures cover a sunlit window, the open
entrance and both lower LODs. The sunlit review recipe only moves the west window
first for the Window camera; authored values and normal lighting are retained.

All 45 exported files match manifest lengths and SHA-256 hashes. Compared with
version 18, materials, collision, doors, layout, all non-timber geometry at LOD0
and LOD1, and the minimal LOD2 geometry are unchanged. The preserved draft has
**37,541 / 1,010 / 134** static triangles. Geometry/material/bake versions are
**19/8/12**. Evidence and the focused source review are under
`artifacts/building-studio/2026-11-11/opening-crosspieces/`.

The signed version-19 canonical app was opened with the original
`wattle-daub/wattle-draft.json`. The door header and both window crosspieces were
reviewed live. It is left on Structure / Window / forced LOD0, with the entrance
open and no unsaved recipe changes. Version 18 is preserved at
`island-building/dist/.previous/pre-opening-crosspieces-20261111-180232/Building Studio.app`.
The crosspiece extension was subsequently committed and pushed as `bad3330`.

## Through-wall timber visible inside

LOD0 timber now extends through the structural wall and projects into the room
by the configured frame projection. Interior caps retain the exterior member
layout and grain, with reverse winding so they render from inside. Inner corner
posts join with matching grain at mitered edges, including when wall thickness
exceeds the beam width. Interior joints are partitioned to avoid competing faces,
and opening cuts continue to respect the casings and sills. All faces share the
existing assignable timber material and colour. LOD1 and LOD2 retain their
existing exterior treatment.

All **55 library and 29 core tests** pass with studio and all targets enabled.
Strict all-target Clippy and formatting checks pass. Added coverage verifies
room-facing winding, full-depth beam sides, coherent inner corners and clear
headers/sills/apertures across all facades, both roof shapes and 8/20/60 cm wall
thicknesses. A near-corner window also verifies that inner corner posts respect
the opening and that their caps do not overlap other members.

Serial Metal captures show the room, entrance and sunlit window at LOD0. The
room has visible posts, braces and crosspieces; the open entrance and clear pane
remain unobstructed. All 45 exported files match manifest lengths and SHA-256
hashes. Compared with version 19, 43 files are byte-identical, including all maps,
doors, collision, layout, LOD1 and LOD2. All non-timber LOD0 geometry also matches.
The preserved draft has **38,311 / 1,010 / 134** static triangles, adding 770 at
LOD0 with no new material draw. Geometry/material/bake versions are **20/8/12**.
Evidence and the focused source review are under
`artifacts/building-studio/2026-11-11/interior-timber/`.

The signed version-20 canonical app was opened with the original
`wattle-daub/wattle-draft.json` and reviewed live. It is left on Structure /
Interior / forced LOD0, with the entrance open and no unsaved recipe changes.
Version 19 is preserved at
`island-building/dist/.previous/pre-interior-timber-20261111-184442/Building Studio.app`.
The through-wall timber extension remains uncommitted for review.

## Solid corner posts and exposed wall-top rails

LOD0 now emits exactly four closed rectangular corner posts, replacing the
separate mitered facade strips and supplementary interior corner pieces. The
posts span both walls and retain vertical grain across their faces. Their
section includes the wall thickness plus beam width and exterior projection;
near-corner openings limit the section so a complete post fits beside its
casing. Shared footprint masks make adjoining rails and braces butt into each
post and carry the matching corner silhouette into LOD1 and the LOD2 atlas.
Four post colliders also cover the new interior projection.

Wattle-and-daub ceilings now sit 6 cm above the wall-top rail where the roof
permits, with a clearance limit for shallow roofs. On the preserved thatched
draft this moves the ceiling underside from 2.72 m to **2.96 m**, a **24 cm**
lift above the original ceiling. The wall-top rail ends at 2.90 m and is fully
visible. Upper plaster, partitions and the straight fireplace breast extend
to the raised slab; ceiling and breast collision follow the new height. The
chimney shaft opening stays clear. Plain-wall ceilings retain their old height.
This prepares the room for the requested later rafters between the window walls.

All **57 library and 30 core tests** pass with studio and all targets enabled.
Strict all-target Clippy, formatting and whitespace checks pass. Tests verify
closed post topology with no diagonal join faces, fitting near-corner casings,
non-overlapping frame caps, through-wall geometry, clear apertures and visible
rails. Ceiling tests cover both roof shapes, all three coverings, shallow and
steep pitches and different wall thicknesses. Ray checks verify upper infill,
the extended breast and matching ceiling collision. The bundled wattle-and-daub
cottage recipe also validates.

Serial Metal captures cover the interior, exterior corner and both lower LODs.
All 45 export entries match their lengths and SHA-256 hashes. Compared with
version 20, 38 files are identical, including all individual material maps,
doors, layout and minimal LOD2 geometry. Only timber, upper interior geometry
and the fireplace breast change in LOD0; only timber changes in LOD1. The atlas
updates to match the wider corner posts. Static triangles are
**38,147 / 1,000 / 134**, with no extra material draw. Geometry/material/bake
versions are **21/8/13**. Evidence and the focused source review are under
`artifacts/building-studio/2026-11-11/solid-corners-ceiling/`.

The current unsaved plain-hut draft was saved as `pre-rebuild-draft.json` in that
evidence folder before rebuilding. The signed version-21 canonical app is open
on the original `wattle-daub/wattle-draft.json`, Structure / Interior / forced
LOD0, with the entrance open and no unsaved recipe changes. The exposed rail and
solid inside corner were reviewed live. Version 20 is preserved at
`island-building/dist/.previous/pre-solid-corners-20261111-191058/Building Studio.app`.
The through-wall work and this extension remain uncommitted on `main`.

## Ceiling flush with bearer tops

The ceiling underside now meets the wall-top bearer directly. The previous
6 cm clearance is removed: the thatched draft's ceiling is **2.90 m**, with no
visible plaster strip or projecting stud ends above the bearer. The shared
ceiling height also updates the slab collision, upper plaster, partitions and
fireplace breast. Shallow-roof clearance remains bounded as before.

All **57 library and 30 core tests**, strict all-target Clippy, formatting and
whitespace checks pass. The existing room ray test now checks the flush height.
The LOD0 Metal capture and live version-22 app both show a continuous ceiling
join against the bearer. The original saved draft is open on Interior / LOD0,
with the entrance open and no unsaved recipe changes. Versions are **22/8/13**.
Evidence and the focused correction diff are under
`artifacts/building-studio/2026-11-11/ceiling-bearer-fit/`. The signed version-21
bundle is preserved at
`island-building/dist/.previous/pre-ceiling-fit-20261111-194222/Building Studio.app`.
The approved interior framing and ceiling work was subsequently committed and
pushed to `main` as `8c12e84`.

## Exposed ceiling joists

LOD0 wattle-and-daub buildings now have exposed joists spanning east to west
between the window walls. The joist positions reuse the actual stud stations
from both walls, including opening jamb stations, with duplicates removed.
Each joist uses the configured timber width for both width and depth, meets the
inner top-plate faces and has its underside flush with the bottom of the top
plates. The ceiling remains at the plate tops. Longitudinal wood grain, colour
and material assignment are shared with the existing framing. Solid joist
collision uses the same bounds as the mesh. Joists stop at the chimney breast
footprint, leaving its masonry and flue clear on any facade. They are generated
only in LOD0; plain-wall buildings keep their existing ceiling.

All **59 library and 31 core tests** pass with studio and all targets enabled.
Strict all-target Clippy, formatting and whitespace checks pass. Added checks
cover stud alignment on both walls, grain along the span, matching underside
and collision elevations across both roof shapes and all coverings, absence
from LOD1, and chimney clearance on all four facades. The saved draft and bundled
wattle-and-daub cottage both validate. The draft has five 14 cm joists with
undersides at **2.76 m**, adding 60 triangles and five colliders without another
material draw. Static triangle counts are **38,207 / 1,000 / 134** and versions
are **23/8/13**.

The LOD0 Metal capture and signed version-23 canonical app were reviewed on the
original saved `wattle-daub/wattle-draft.json`. Joists meet the plate bottoms
cleanly and the ceiling stays flush above them. The app is left on Structure /
Interior / forced LOD0, with the entrance open and no unsaved recipe changes.
Version 22 is preserved at
`island-building/dist/.previous/pre-ceiling-joists-20261111-200701/Building Studio.app`.
Evidence and the focused source review are under
`artifacts/building-studio/2026-11-11/ceiling-joists/`. The new joist extension
remains uncommitted for review.

## Even ceiling-joist spacing

The joist grid now uses room depth and the existing bay-spacing setting instead
of stud or window locations. Joists have equal clear gaps between them and at
both end walls, with centre spacing bounded by the bay-spacing setting. The grid
is generated directly as an iterator without collecting or sorting stud rows.
Moving or resizing an opening does not change it. Plate-bottom alignment,
material, grain, collision and chimney clearance retain their previous behavior.

All **59 library and 31 core tests**, strict all-target Clippy, formatting and
whitespace checks pass. Updated spacing coverage checks equal internal/end gaps,
room depths from 3 to 40 m, several bay spacings and independence from opening
edits. The LOD0 Metal capture and signed version-24 app were reviewed with the
original saved thatched draft. It now has three 14 cm joists at 1.1725 m centres
with 1.0325 m clear gaps, including the end gaps. Static triangles are
**38,183 / 1,000 / 134**, with 58 colliders and unchanged material draws. Versions
are **24/8/13**. The app is left on Structure / Interior / forced LOD0, entrance
open, with no unsaved recipe changes. Version 23 is preserved at
`island-building/dist/.previous/pre-even-joists-20261112-072744/Building Studio.app`.
Evidence and the focused revision are under
`artifacts/building-studio/2026-11-12/even-joists/`. The joist extension remains
uncommitted on `main`.

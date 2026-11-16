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
`artifacts/building-studio/2026-11-12/even-joists/`. The approved joist extension
was subsequently committed and pushed to `main` as `7fb8d3c` before the door work.

## 18th-century frame-and-panel doors

Openings now have a per-door **18th-century panels** option alongside **Plank**.
The Georgian-style six-panel leaf has solid stiles and rails, smaller upper
panels, taller middle/lower panels, moulded borders and raised fields on both
faces. Its proportions draw on the
[Georgian Group's historic front-door examples](https://georgiangroup.org.uk/wp-content/uploads/2024/07/GGM_Brooking_09_Front_doors.pdf).
It uses the existing door colour/material, with vertical grain on stiles and
panels and horizontal grain on rails. Hinges, clearances, knobs, opening action
and collision retain their previous behavior. Older recipes default to Plank;
only non-default styles add a `door_style` property to saved JSON.

The LOD0 leaf has **564 triangles**, including both detailed faces, within the
existing 55 mm thickness. LOD1 uses a **126-triangle** closed exterior proxy;
LOD2 keeps the minimal shell and bakes the panel relief, occlusion and oriented
material into the atlas. No material draw is added. Static triangle counts for
the thatched draft are **38,183 / 1,124 / 134**, excluding the separately hinged
LOD0 leaf and hardware. Geometry/material/bake versions are **25/8/14**.

All **61 library and 32 core tests** pass with studio and all targets enabled.
Strict all-target Clippy, formatting and whitespace checks pass. Coverage checks
legacy recipe defaults and round trips, both detailed faces, moulding relief,
usable material UVs, winding and simplified proxies on all four facades and
both hinges. The portable export test also uses an assigned wooden-planks
material and verifies regenerated door geometry, style and material dependencies.
The manual export's 45 manifest entries match their sizes and SHA-256 hashes.
Serial Metal captures were reviewed for the closed exterior and open interior
at LOD0 and the closed exterior at LOD1 and LOD2.

The signed version-25 canonical app was rebuilt and reviewed live. Selecting
the new style, saving a review copy, Undo back to Plank and Redo back to the
saved panel style all work, including the dirty-state indicator. The panelled
entrance opens and closes with its knob. The app is left on
`panel-door/panel-door-review.json`, Openings / Entrance / forced LOD0, with the
door closed and no unsaved recipe changes. The original saved wattle draft is
preserved. Version 24 is backed up at
`island-building/dist/.previous/pre-panel-door-20261112-075545/Building Studio.app`.
Evidence and the focused source review are under
`artifacts/building-studio/2026-11-12/panel-door/`. The new door option remains
uncommitted on `main` for review.

## Door leaf seated within the frame

Both door styles now sit **55 mm farther back** within the existing lining,
with their inside face flush with the room-side frame. The hinge axis is at
that inside face; the leaf and both knobs are offset together in hinge-local
coordinates. This preserves the 3 mm side/top and 6 mm floor gaps while keeping
the leaf clear of the jamb during its inward swing. No frame or wall geometry
changes. Moving collision, picking and player-blocking queries share the same
leaf bounds. Exported descriptors include a signed `depth_offset` for collision;
the exported mesh vertices already include it, and older descriptors default
to zero.

All **62 library and 32 core tests**, strict all-target Clippy, formatting and
whitespace checks pass. Expanded fitting coverage checks both styles, both
hinges, all four facades and 12/20/45 cm wall thicknesses, including closed-depth
alignment and clearance throughout the swing. Query tests cover offset sphere,
capsule and ray hits, the rotated collider and legacy descriptor loading.
The portable export's 45 manifest entries match their sizes and SHA-256 hashes.
Only the prototype, door descriptors, leaf OBJ and hardware OBJ change compared
with the prior panel-door export; all static LOD meshes and material/atlas maps
are byte-identical. Versions are **26/8/14**, with unchanged triangle counts.

Metal captures show the fitted closed door from outside and inside and the
open leaf from inside. The signed version-26 canonical app was reviewed live,
including a close interior check and a complete open/close cycle. It is left on
the original saved `panel-door/panel-door-review.json`, Openings / Interior /
forced LOD0, door closed, with no unsaved recipe changes. Version 25 is preserved
at `island-building/dist/.previous/pre-door-frame-fit-20261112-083253/Building Studio.app`.
Evidence and the incremental correction diff are under
`artifacts/building-studio/2026-11-12/door-frame-fit/`. The panel-door work and
this correction remain uncommitted on `main`.

## Wooden floor and ceiling boards

The new reusable `island-rs/texture-recipes/wooden-floorboards.json` material
uses 20 cm wide, 2.4 m long timber boards with staggered end joints, 3 mm seams,
1.8 mm bevels, subtle grain and board-to-board colour variation. It supplies
albedo, normal and occlusion maps through the existing shared material evaluator.
Floors and ceilings now use it by default. Each surface has its own palette
colour and optional JSON recipe selector. A dedicated ceiling slot keeps the
plaster walls, partitions, fireplace surround and joists on their existing
materials. Clearing a custom reference restores the built-in board recipe.

Floor and ceiling charts use common room coordinates, keeping the ceiling boards
continuous around the flue opening and perpendicular to the east-west joists.
Slab bounds and collision are unchanged. Ceiling cutaway also removes the new
slot. Preview maps use 256 px, and authored maps use up to 512 px. All three
static triangle counts remain **38,183 / 1,124 / 134**; separating the ceiling
adds one LOD0 material draw. Geometry/material/bake versions are **27/9/14**.

All **63 library and 33 core tests**, strict all-target Clippy, formatting and
whitespace checks pass. Coverage includes old recipes, independently tinted
surfaces with unchanged normal relief, continuous UVs around flues on all four
facades, material selection/Undo, relocated document dependencies and portable
exports containing both floor and ceiling recipe snapshots. All 51 entries in
the built-in-material export and 53 in the explicitly assigned export match
their lengths and SHA-256 hashes. Lower LOD meshes, moving doors, collision and
the exterior atlas remain byte-identical to the preceding fitted-door export.

Metal captures and the signed version-27 canonical app show both finished
surfaces. Both JSON pickers, Save, Undo back to the built-in ceiling and Redo
back to the saved assignment were checked live. The app is left on
`wooden-boards/boards-review.json`, Materials / Interior / forced LOD0, with the
entrance open and no unsaved recipe changes. Both surfaces explicitly reference
the new source recipe in this review copy. The prior saved building drafts are
preserved. Version 26 is backed up at
`island-building/dist/.previous/pre-wooden-boards-20261112-103756/Building Studio.app`.
Evidence and the incremental source review are under
`artifacts/building-studio/2026-11-12/wooden-boards/`. This work remains uncommitted
on `main` alongside the reviewed door changes.

## Regular plank joints aligned with joists

The floorboard recipe now enables `regular_stagger`: all even board rows share
end joints, and all odd rows have a half-length offset. Full boards have equal
lengths. The shared material evaluator retains random staggering for older
recipes without this flag. Regular staggering requires an even board count so
the texture repeats seamlessly; Material Studio exposes the new checkbox and
reports invalid counts through its normal recipe diagnostics.

In timber-framed buildings, floors and ceilings share the joist grid's origin.
Regular wooden materials fit each full board to two joist bays, placing their
end joints over alternate joist centres. Joist spacing and geometry are unchanged.
The saved draft has **2.345 m** full boards with a **1.1725 m** half offset.
The standalone material keeps its authored 2.4 m board length. Normal/AO relief
is evaluated using the fitted physical dimensions, and portable exports preserve
both the source recipe and the effective material scale.

All **97 shared procedural-material tests**, **63 Building library and 34 core
tests**, and **45 Material Studio library plus 2 CLI tests** pass. Strict Clippy
for all three affected crates, formatting and whitespace checks pass. New tests
check alternate-row alignment across seeds, half-length offsets, wrapping and
odd-count rejection. Building coverage compares texture joints against actual
joist collider centres across different room depths and bay spacings.
All 53 portable-export entries match their lengths and SHA-256 hashes. Lower
LOD meshes, doors, collision and all maps except floor/ceiling remain byte-identical
to the preceding board export. Triangle counts remain **38,183 / 1,124 / 134**;
geometry/material/bake versions are **28/10/14**.

The Metal render and signed version-28 Building Studio were reviewed on the
original saved `wooden-boards/boards-review.json`. It is left on Structure /
Interior / forced LOD0, entrance open, with no unsaved recipe changes. Material
Studio was also rebuilt and signed with the new control and recipe. Both prior
bundles are preserved in their respective `dist/.previous/` directories under
`pre-regular-board-joints-20261112-110629/`. Evidence and the incremental source
review are under `artifacts/building-studio/2026-11-12/board-joints/`. The changes
remain uncommitted on `main`.


## Visible floorboard end seams

The regularly aligned 3 mm end joints could fall entirely between pixel centres
in the 256 px editing maps. The wooden-plank evaluator now integrates each end
seam and its bevel across the physical texel footprint, preserving narrow joints
in albedo and relief without moving the joins or widening their authored gaps.
The floor shows alternating plank end joints; supporting floor joists stay
concealed. Ceiling end joints retain their phase over the exposed ceiling joists.
Continuous wooden materials without end joints retain their previous maps.

All **98 shared material tests**, **63 Building library and 34 core tests**, and
**45 Material Studio library plus 2 CLI tests** pass, along with strict Clippy
for all three crates and formatting/whitespace checks. The regression samples
actual raster pixels at 256, 512 and 1024 px, checking both alternating rows,
tile-edge wrapping, visible albedo contrast and recessed relief.

Metal captures at editing and authored resolutions show the restored floor
joints and ceiling joints concealed by joists. All 53 exported files match
their lengths and SHA-256 hashes. Every mesh, door, collision file and all
non-board maps are byte-identical to the preceding export. Triangle counts
remain **38,183 / 1,124 / 134**; geometry/material/bake versions are **28/11/14**.
Both studio bundles have been rebuilt and signed, with prior bundles preserved
in `dist/.previous/pre-visible-plank-joints-*/`. Building Studio is version 29.
Evidence and the focused correction diff are in
`artifacts/building-studio/2026-11-12/floor-plank-joints/`. These changes remain
uncommitted on `main`.

The signed version-29 Building Studio was checked live on the original saved
`wooden-boards/boards-review.json`, with visible floor plank joints and concealed
ceiling end joints. It is left on Structure / Interior / forced LOD0, entrance
open, with no unsaved recipe changes. The recipe file itself is unchanged.


## Terrain foundations and hideable entrance stairs

The reviewed panel doors and regular floor/ceiling boards were committed and
pushed as `c2b61fe`. Foundations are now separate from the wooden floor, with
an independent colour and material recipe (coursed stone by default). Their
footprint projects 8 cm beyond the walls, and depth is the full footprint
diagonal times tan(30°), plus a default 15 cm burial margin. Native floor top
stays Z=0; place a level building with the highest ground point anywhere inside
the foundation at that elevation. The bound covers all terrain gradients up
to 30°, including an interior high point rather than only a corner.

Every exterior door gets an 80 cm clear landing and solid steps with 20 cm rise
and 24 cm tread. The length accounts for terrain continuing downhill beyond
the door and checks both toe corners against every possible foundation high
point. The review hut has a **4.789 m** foundation depth and **62 steps**, with
**15.76 m** total outward reach; terrain conceals unused lower steps. The
landing stands 2 mm above the permanent plinth cap to avoid coplanar drawing
while retaining the existing door clearance. LOD2 uses a 24-triangle ramp
proxy; all foundation/stair surfaces share one set of baked stone maps. Static
triangle counts are **38,957 / 1,898 / 176**; versions are **29/12/14**.

Recipe controls under Structure include outset, burial margin and whether to
generate stairs. Inspection's per-placement stair switch updates rendering,
capsule/sphere/ray collision and walking support together. The setting survives
scene-state snapshots, and older states default to showing the generated stairs.
The studio walker can step up and down the treads. The support allowance remains
in world metres under rotated or nonuniformly scaled placements. Deep buried
bounds remain in collision/culling descriptors, while projected LOD size and
normal overview framing use the above-ground building. The Foundations camera
frames the complete access geometry; slope/direction controls alter only the
inspection terrain, not the authored recipe.

All **64 library, 34 core and 4 foundation tests** pass. Subsequent focused
scaled-support and camera checks pass, together with strict all-target Clippy,
formatting and whitespace checks. Terrain tests cover square and narrow 3–40 m
footprints, all four door facades, offset doors, all 360 integer slope headings,
and high points throughout the foundation footprint. Collision tests verify
hiding stairs, state migration and visibility, and portable dependency tests
include assigned foundation recipes and Save As relocation.

Metal captures cover 30° downhill terrain at all three LODs, a diagonal 30°
slope and hidden stairs on near-level ground. All **59 exported files** match
their lengths and SHA-256 hashes. Every pre-existing material map, door and
mesh is byte-identical to the preceding export; structural collision differs
only by the added foundation. Stair collision is separate and must be switched
with the stairs material group. `foundation.json` gives buried bounds, while
`stairs.json` records each entrance's reach and step dimensions.

Evidence and the focused source diff are under
`artifacts/building-studio/2026-11-12/foundations/`. New foundation and stair work
remains uncommitted on `main`, with unrelated runtime and crash files preserved.

The signed version-30 canonical Building Studio was reviewed live on the saved
`foundations/terrain-review.json`. The slope preview reached exactly 30°, stair
visibility removed its mesh and 63 colliders together, and distant/detailed LOD
switching worked. It is left on Structure / Foundations / forced LOD0 with
stairs visible, slope 30°, direction 0°, and no unsaved recipe changes. The
previous version-29 bundle is preserved at
`island-building/dist/.previous/pre-foundations-20261112-124739/Building Studio.app`.


## Along-wall entrance stairs

The reviewed foundations and outward stairs were committed and pushed as
`227cdfb`. **Structure → Stair direction** now offers **Outward** and **Along
wall**, stored as `foundation.stair_run`. Older recipes default to Outward.
Along-wall stairs turn from a square landing and descend parallel to the facade
toward the farther corner beside the door. Positive facade offsets choose the
left side; negative offsets choose the right. A centred door consistently uses
the right-hand side when viewed from outside. Every facade uses its own tangent.

Near/middle meshes, the distant ramp proxy, colliders, bounds and 30° terrain
reach share the same borrowed layout calculation. Side flights sit outside the
plinth and meet their landings. Stair visibility and support remain independent
per placement. Exported `stairs.json` now records orientation, flight direction,
first-tread origin and landing bounds, alongside the existing dimensions.
Geometry/material/bake versions are **30/12/14**; the canonical app is version 31.

All **65 library, 34 core and 6 foundation tests** pass, with strict all-target
Clippy, formatting and whitespace checks. Coverage includes both directions,
all four facades, centred and offset doors, 360 slope headings, narrow/square
footprints, every tread's support at all LODs, hidden collision and portable
regeneration of the along-wall option. Metal captures show both offset signs,
LOD0/LOD2 on a 15° slope and worst-case 30° ground descending along the flight.

Both portable exports have **59 verified files**. The preserved outward layout
retains all non-stair meshes, maps, doors and structural collision byte-for-byte;
its stair collider coordinates differ by at most 0.3 micrometres from the generic
flight-frame arithmetic, with unchanged step count and reach. The off-centre
side-stair review has 46 steps, 11.765 m total reach, and static triangle counts
**38,529 / 1,666 / 176**. The normal terrain preview conceals the unused lower
steps. Source review and evidence are under
`artifacts/building-studio/2026-11-12/along-wall-stairs/`. New side-stair work
remains uncommitted on `main`; unrelated runtime and crash files are preserved.

The signed version-31 canonical app was reviewed live on the saved
`along-wall-stairs/along-wall-review.json`. Selecting Outward rebuilt the flight,
and Undo restored Along wall and the clean saved recipe. It is left on Structure
/ Foundations camera / forced LOD0, slope 15°, direction 0°, stairs visible and
door closed. The 0.65 m right offset correctly sends the stairs left along the
longer wall span. The prior version-30 bundle is preserved at
`island-building/dist/.previous/pre-along-wall-stairs-20261112-130527/Building Studio.app`.


## Material-unit authoring · 12 November 2026

The reviewed along-wall stair option was committed and pushed as `8daa942`.
New material-unit work remains uncommitted on `main`; unrelated runtime changes
are preserved. Recipes continue to store physical metres with no schema change.
The editor now offers Material units / Metres, a per-tab reference surface, live
U/V module sizes, explicit Snap controls and a Reload grid control. Modules follow
custom recipe physical sizes and board/block/course/bundle counts. Opening offsets
are authored from their left edge. Floor/ceiling footprint controls use clear room
sizes; the foundation grid uses plinth dimensions including the outset. Roof grid
controls use slope length and overall roof length. Small construction clearances
remain metric. Existing off-grid dimensions are retained until an explicit edit.

Walls and gables share edge-based charts, board widths start at the inside wall,
and masonry courses share floor-height phase. Chimney stack charts and distant
bakes agree. Foundation/stair charts no longer restart at their buried bottoms.
Joist-aligned staggered end joints are retained. Slate relief uses the active
material course pitch, with bounded coarse relief on integer course multiples.
Geometry/material/bake versions are **31/12/15**, app bundle version **32**.

All **70 library, 34 core, 6 foundation and 2 material-unit tests** pass (112 total),
with strict all-target Clippy, formatting and whitespace checks. Coverage includes
reading custom physical sizes, rejecting invalid recipes, signed/edge-based
snapping, unchanged dimensions on viewing, real egui Snap/Undo/Redo interaction,
keyboard input retained until commit, all-facade wall/foundation UVs, matching
LOD2 wall/chimney sampling and custom slate course relief. LOD0 and LOD2 Metal
captures retain the existing building silhouette, doors and stairs, with clear windows at LOD0.
Portable export contains 59 length/SHA-256 verified files.

Evidence and the focused patch are in
`artifacts/building-studio/2026-11-12/uv-units/`.

The final signed canonical app was checked live: choosing Floor boards displayed
0.200 m U units and 2.400 m V units without changing the saved recipe. Snapping
4.600 m inside depth produced two V units / 4.800 m, with Undo restoring the clean
6.000 × 5.000 m saved footprint. In the final build, typing 29.2 U retained the
original width until Enter, then produced 29 U / 5.800 m clear width and a 6.200 m
outside width. Undo restored 28 U. The preserved pre-feature app is
`island-building/dist/.previous/pre-uv-units-20261112-143451/Building Studio.app`.


## Solid step slabs and fireplace hearth · 12 November 2026

LOD0/1 stair treads and landings now have 45 mm solid stone caps, with 15 mm
exposed-edge/nosing overhangs. Original walking elevations and threshold clearance
are retained. Caps fit against the facade/plinth, and the reach calculation now
includes their toe corners. Their collision includes the overhang and follows the
existing per-placement stair visibility. The LOD2 landing and thin ramp cap use the
same material. The LOD0 hearth is one uninterrupted stone box, including the floor
inside the firebox. Structural hearth collision remains independent of stair state.

`materials.stone_slab` / `stone_slab_recipe` are backward-compatible palette fields.
The built-in `island-rs/texture-recipes/stone-slab.json` is subtle layered stone noise
with no masonry joints. The Stone slabs controls independently assign colour/JSON.
`StoneSlab` and `StairTreads` are separate mesh groups sharing one set of generated
maps/handles, so hiding tread geometry cannot hide the hearth. Source dependencies
relocate with Save As, export portably and participate in prototype identity.
Geometry/material/bake versions are **32/13/15**, bundle version **33**.

All **72 library, 34 core, 6 foundation, 2 material-unit and 2 stone-slab tests**
pass (116 total), with strict all-target Clippy, formatting and whitespace checks.
Tests cover all facade/orientation combinations, nondegenerate pieces with zero
burial margin, original step heights, overhang support/collision, all-LOD visibility,
hearth persistence, shared map handles, independent tint/normal relief, legacy
recipes, picker Undo, source relocation and complete portable regeneration.
The 30-degree terrain suite still checks every toe corner over 360 headings.

Metal captures show solid tops on the stairs and a continuous hearth/firebox floor.
The portable export has **62 SHA-256/length-verified files**, with static LOD triangle
counts **39,115 / 2,254 / 200** for the review cottage. Evidence and focused/new-work
patches are in `artifacts/building-studio/2026-11-12/stone-slabs/`.

The previously open Studio contained unsaved roof edits (rise 3.0785012 m,
building depth 5.966667 m). They were preserved through Save As to
`stone-slabs/preserved-building.json` before replacing the app. UV authoring work
and this slab change remain uncommitted on `main`; unrelated runtime work is preserved.

The signed canonical version-33 app was reviewed live on the preserved draft.
The new Stone slabs JSON picker successfully assigned `stone-slab.json`; Undo
restored the built-in surface and clean saved recipe. The hearth remained visible
with entrance stairs hidden. Stair visibility was then restored, along with the
open entrance, Materials controls, forced LOD0 and 15-degree terrain preview.
The pre-feature version-32 app is preserved at
`island-building/dist/.previous/pre-stone-slabs-20261112-154457/Building Studio.app`.

## Floor-relative grub shader · 12 November 2026

Added optional `grub_filter` recipe settings and a Grub inspector tab. Existing
recipes default to disabled. Three scales of seeded, building-local 3D noise add
broad dirt blotches; a separate patch mask and the final normal-mapped surface
normal favour moss on upward faces. Influence is `1 / (1 + exp2(height / falloff))`:
exactly 50% at floor level, stronger below and weaker above. Strengths, colours,
patch size and height falloff are editable. Filter colours alter PBR albedo and
increase roughness while preserving opacity, cutouts and normals.

`BuildingGrubPlugin` and `compile_building_with_grub` provide the optional extended
PBR material at all LODs. Standard material/image handles are shared; no extra
texture maps are allocated. Glass and separate door hardware retain standard
materials. A second vertex UV channel carries floor-relative height, including
hinged door leaves. The original `compile_building` API remains standard PBR;
consumers opt into the plugin and new compiler function to render this filter.
Portable packages now include the settings/coordinate descriptor
`grub-filter.json` and the shared noise/masking functions in `grub-filter.wgsl`.
Their base maps and LOD2 atlas remain unfiltered. Geometry/material/bake versions
are 32/14/15 and the signed macOS app is build 34.

Validation: all **75 library, 34 core, 6 foundation, 2 grub-filter, 2 material-unit
and 2 stone-slab tests** passed with the studio feature (121 total). All 87
no-default-feature tests also passed. Strict studio/all-target Clippy, crate
formatting and whitespace checks passed. Tests cover legacy defaults, settings
round trips and validation, shader parsing, the monotonic 50%-at-floor profile,
unchanged maps/geometry/collision, shared image handles, masked material alpha,
glass exclusion and static/hinged floor heights across every LOD.

Metal offscreen renders cover clean versus filtered entrance views, foundations
and LOD2. An exaggerated red/green diagnostic verified the live shader's dirt
and upward moss masks. The live app's Grub toggle, falloff control and Undo were
exercised. Portable export verified all 64 file lengths and SHA-256 hashes. The
new app's signature, Mach-O UUID and all 15 file-backed sections match the current
debug executable; its bundled README matches the source. Build 33 is retained at
`island-building/dist/.previous/pre-grub-filter-20261112-162952/Building Studio.app`.

Evidence and focused/cumulative review patches are in
`artifacts/building-studio/2026-11-12/grub-filter/`. The user's prior saved building
was used for the new review recipe; its source was preserved. UV authoring, stone
slabs and this filter remain uncommitted on `main`; unrelated runtime changes are
preserved.

A subsequent live Save published the review JSON successfully, then stalled
opening its parent directory for the optional durability sync. The process sample
is retained as `app-sample.txt`. macOS now skips that extra directory open while
retaining the temporary file sync, source-conflict checks and atomic publication;
other platforms retain directory syncing. All five document tests and strict
Bevy-only and studio Clippy passed after the correction. In the repackaged app,
Save completed with the visible "Recipe saved" status and the preview remained
responsive. The review recipe was verified intact on disk.

## Broad and fine grub layers · 12 November 2026

Replaced the three soft noise octaves with two independently contrasted layers:
`patch_size` controls the broad patches, `fine_patch_size` controls the fine
breakup, and `fine_mix` combines them. Legacy filter recipes retain their broad
size and falloff, gaining a 0.18 m fine layer at 35% contribution. The Grub tab
exposes all three controls and explicitly identifies the building floor midpoint.

The existing anchor was already the building floor (native Z=0 / Bevy Y=0),
independent of the terrain preview. The previous multiplication weakened an
already soft mask. The corrected strength is `clamp(2 * height_influence * noise,
0, 1)`: neutral noise gives 50% at the building floor, while individual blotches
retain full contrast there and remain visible above it. Height influence still
increases downward and fades upward. Normal-guided moss and glass/hardware
exclusions remain intact, with no additional textures or geometry.

All 122 studio-feature tests and strict all-target Clippy passed. Tests cover
legacy defaults and retained parameters, invalid fine-layer settings, exact GPU
uniform packing, the physical floor origin, all LODs and hinged leaves. WGSL
validation, formatting and whitespace checks passed. Metal captures compare the
old and new entrance views, isolate each layer, and inspect LOD2; the portable
export verifies all 64 file lengths and hashes, including the updated shader and
five-vector descriptor. Material version is 15; the signed macOS app is build 35.

The user's unsaved 5 m falloff was saved separately before replacing the app and
retained in the new review recipe. Evidence and focused/cumulative patches are in
`artifacts/building-studio/2026-11-12/grub-two-layers/`. Changes remain uncommitted
on `main`; the git index and unrelated runtime work are untouched.

Live build 35 verified fine-contribution slider editing, gesture Undo back to
35%, and saving both noise scales with the retained 5 m falloff. The Grub tab is
left open; the focused diff and manifest are open in the Island-experiment editor.

## Dedicated exterior grub groups · 13 November 2026

Restricted weathering to a single material allowlist: foundations, stair bodies
and treads, thatch roof/end/shell groups, and exterior wattle-and-daub panels.
Timber, trim, doors, chimneys/fireplaces, hearth slabs and all interior groups
retain their original materials. Shared surfaces are excluded without adding
fragment-side masks. The mixed LOD2 facade atlas stays clean to protect its baked
timber and joinery; dedicated foundation, stair and thatch groups remain eligible
at every LOD. Portable exports carry the same allowlist and exclusions.

All 124 studio-feature tests, strict all-target Clippy, formatting and whitespace
checks passed. Coverage checks the allowlist for wall/roof styles, clean hinged
doors and interior/shared groups, and independent weathering of stair treads
while the indoor hearth shares their original stone texture. Metal captures with
strong red dirt and green moss verify the exterior groups while the interior,
timber, joinery and fireplace stay clean. Normal-colour interior and LOD2 captures
also completed. The portable export verifies all 64 file lengths and hashes.

Material version is 16; the signed macOS app is build 36. A separate signed
Building Studio Exterior Review app loads the valid exterior review recipe with
the current weathering settings. The user's original build-35 window and its
unsaved invalid partition draft remain open and untouched. Evidence is in
`artifacts/building-studio/2026-11-13/exterior-grub/`. Changes remain uncommitted
on `main`, preserving the empty index and unrelated runtime work.

## Shared opening dimensions and automatic layouts · 13 November 2026

Added building-wide window width/height/sill and door width/height settings, plus
optional automatic facade layouts. New Studio presets and
`BuildingRecipe::procedural(width, depth)` generate entrances and evenly spaced
windows from the footprint. Generation reserves corners, fireplaces and partition
ends; a blocked central entrance moves into a clear span. Stable opening IDs
support partial field overrides and suppression. Temporarily absent slot overrides
survive resizing. Manual mode retains the generated layout and permits arbitrary
additions/removals; legacy recipes keep their explicit sizes and layout.

New internal doorways also inherit the global door sizes, with a per-partition
override. Resolution borrows unchanged manual recipes and otherwise owns an
authoring snapshot, leaving the source document untouched. The prototype's resolved
recipe supplies all geometry, LODs, collision, stairs and atlas generation.

All 136 studio-feature tests, strict all-target Clippy, formatting and whitespace
checks passed. New tests cover legacy decoding, shared and overridden dimensions,
deterministic layouts from 3 to 40 metres, fireplace/partition clearances, dormant
overrides, invalid settings/IDs, last-door suppression, exact LOD/collision/stair/
atlas parity with an equivalent manual layout, regeneration, history and passive
editor rendering. All three Studio presets and the review recipes validate.

Metal captures inspect generated thatched buildings at the original and expanded
footprints, LOD0 interiors and LOD2 exteriors. The portable package verifies all
64 file sizes and hashes. Geometry version is 33; material/bake versions remain
16/15. Signed macOS build 37 is packaged with the updated README.

Live review changed the shared window width from 1.4 to 1.6 metres, checked Undo
and Redo restoring the size/count, then assigned a 1.25 m panel-door override while
retaining the 1.15 m global door width. Save As completed with "Recipe saved";
the resulting `interactive-review.json` preserves the global sizes and override
and validates from the CLI. A separate Openings Review app preserves the original
unsaved partition draft in its existing window. Evidence and focused/cumulative
patches are in `artifacts/building-studio/2026-11-13/automatic-openings/`.
Changes remain uncommitted on `main`; the actual index and unrelated work are
untouched.

## Multiple storeys and internal stairs · 13 November 2026

Added one to six occupied storeys, keeping wall height as the per-storey room
height. Upper floor elevations include floor thickness. Each storey resolves its
own openings and partitions; automatic windows repeat with stable storey-suffixed
IDs, while entrances stay on the ground floor. Floor-local geometry buffers move
into one prototype, with one roof and raised chimney stack. Half-timbered floor
bands join the frame and match their distant atlas coverage.

Multi-storey recipes resolve a clear U-stair position/orientation or report why
the footprint cannot accommodate it. Equal risers, turn landings, floor landings,
balusters and handrails use the existing timber material without weathering.
Upper slabs and lower-storey joists have real cutouts. Occupied floor and internal
stair support is separate from hideable entrance-stair collision and remains
active at every LOD. Interior stair meshes appear at LOD0 only. Layout exports
include floor heights and landing/stair metadata; `support-collision.json` contains
the walkable floor and internal stair supports.

All 148 studio-feature tests, strict all-target Clippy, formatting and whitespace
checks passed. Tests cover legacy one-storey decoding, two/three/six storeys,
window overrides, roof bounds, floor cutouts, landings, conflicting placements,
partitioned cottages, both stair axes, and portable export/regeneration. Player
capsules climb and descend the complete three-floor route at every LOD, including
low thatched timber rooms, with entrance stairs hidden. Camera tests verify clear,
supported positions for upper-floor Interior, Window and Internal stairs views.

Metal captures review two/three-storey exteriors, upper rooms, internal stairs,
and LOD1/2. The final portable package verifies all file lengths and hashes.
Geometry/material/bake versions are 34/17/16; the signed macOS app is build 38.
Evidence and focused/cumulative patches are under
`artifacts/building-studio/2026-11-13/storeys/`.

Live review exposed a linked-material read blocking inside a UI frame. A process
sample identified `StudioDocument::open` / `validate_dependencies` waiting in
`open`. Recipe loading now runs on the IO task pool, with revision/serial guards
that retain edits made while a file is loading. Two additional tests check stale,
successful and failed load boundaries. The original unsaved partition draft
remains open in its original process; only the separate test window was restarted.
The final build opened the portable three-storey recipe successfully. Live edits
rebuilt it with two storeys, Undo restored three, and the Internal stairs and
upper-storey Interior views showed the open stairwell, landings and guardrails.
Both signed bundles match all 15 file-backed executable sections of the tested
binary; all 65 exported file lengths and SHA-256 hashes match the manifest.

Floor-local recipe copies deliberately isolate floor-relative openings and roof
settings for the existing geometry helpers; a borrowed view would require those
helpers to accept a second coordinate/configuration model. Mesh buffers are moved
into the final prototype and generated textures are shared across storeys.
All changes remain uncommitted on `main`, preserving the empty index and unrelated
runtime work.

## Open-riser timber stairs · 13 November 2026

Internal flights now have open risers and two continuous solid timber stringers.
Treads fit between the stringers, with their housed end faces omitted. The final
tread meets a flat stringer end at the landing. Railing posts start on the stringer
tops, and handrails follow the pitch and landing transition. The timber material,
landings and independent internal stair support remain in use.

All 148 studio-feature tests, strict all-target Clippy, formatting and whitespace
checks pass. Existing capsule tests climb and descend the full three-floor route
at every LOD, for both stair axes and low thatched timber rooms. Matching Metal
captures and the live Internal stairs view confirm the open gaps, housed treads
and continuous stringers. The three-storey review recipe falls from 58,000 to
57,776 LOD0 triangles, saving 224; LOD1/2 remain at 5,545/200 triangles.

Geometry/cache version is 35, with material/bake versions 17/16. Signed macOS build
39 matches all 15 executable sections of the tested binary in both the canonical
and separate Stairs Review bundles. Earlier app processes and the original unsaved
draft remain open. Evidence and focused changes are under
`artifacts/building-studio/2026-11-13/open-riser-stairs/`; work is uncommitted on
`main`, with the actual index empty and unrelated runtime work preserved.


## Enclosed stairwell with floor doors · 13 November 2026

Multi-storey staircases now default to an enclosed interior stair room, with one
outward-opening door from each floor landing. Structure offers an enclosure toggle
and plank/frame-and-panel door styles. Door dimensions follow the shared building
defaults; leaves and hardware reuse the door materials and interaction system.
The enclosure uses interior surfaces without weathering. Stair-room bounds and
floor doorway approach points are exported, with stable `stairwell-door-N` leaves
marked `exterior=false`. Internal door meshes inherit LOD0 visibility; enclosure
and moving-door collision remain active at every LOD. One continuous stair room
is added to the resolved room count.

Placement reserves wall thickness and clear room-side door swing/approach space,
then searches intermediate positions around doors, partitions and the chimney.
Enclosure walls meet the top ceiling and close the intermediate floor bands.
Perimeter guards are omitted against enclosing walls; flight railings and the
upper landing's open-edge guard remain. Preview room lighting follows the inspected
floor, and Stair door inspection chooses a position outside the leaf sweep when
one is available.

The 151-test studio suite passed. Focused stairwell, storey and camera tests were
rerun after visual adjustments; strict all-target Clippy, formatting and whitespace
checks passed. Both stair axes have a supported capsule route through every floor
door: closed leaves block passage, and open leaves allow passage at every LOD.
Portable round-trip tests retain the new layout metadata and independent doors.
Metal captures review the enclosure and closed/open leaves; the live app loads the
three-storey example and responds to the upper-floor door control. Its Internal
stairs view is left open with the enclosure enabled.

The signed macOS app is build 40, with geometry/material/bake versions 36/17/16.
Canonical and final review bundles match the tested executable's file-backed
sections. Evidence and focused/cumulative diffs are in
`artifacts/building-studio/2026-11-13/stairwell-room/`. Changes remain uncommitted
on `main`; the actual index is empty and unrelated runtime work is preserved.


## Stairwell sides reuse nearby exterior walls · 13 November 2026

Enclosure sides closer than one authored exterior-wall U module to the inside
face of a parallel external wall are omitted. Exact one-module gaps retain their
partition. The clear room limits expand to the reused facades; remaining walls
extend to those faces, preserving the existing glazing and timber frame. This is
resolved once in stair-local coordinates and used by rendering, collision,
doorways and exported enclosure metadata.

Each floor door moves to an accessible retained partition. Automatic placement
can face the flights either way along its selected axis, keeping entrances and
the chimney clear. Open edges beside newly shared floor strips retain guardrails.
Layouts without an accessible retained partition or clear door swing report the
placement diagnostic; general centred-entrance fixtures use 7 x 6 m, while the
compact 6 x 5.966667 m off-centre cottage remains covered explicitly.

Wall modules resolve relative to the owning material/recipe directory in both
validation and generation. File open/save and explicit Validate use that base;
UI preview prechecks perform authoring validation before the background worker
resolves linked material files and the full layout. Exported `enclosure` records
local clear limits, near/right/far/left shared sides, the U module in metres and
the selected door wall. `reversed` rotates the stair basis 180 degrees. Old layout
fields decode with their existing enclosure/basis defaults.

The full 154-test studio suite passed. All 15 stairwell/storey/shared-wall tests
were rechecked after final guardrail changes; strict all-target Clippy, formatting
and whitespace checks passed. Threshold tests cover all four sides on both axes
just below, at and above one authored U unit. Tests also check wall extensions,
relative material references, preserved glass, closed/open floor doors at all
LODs, and climbing/descending the compact cottage's complete reversed route.

Metal captures show the exposed facade/windows, the open relocated floor door,
and upper-floor enclosure joins. The live signed build 41 is open on the shared
stairwell's Internal stairs view. Geometry/material/bake versions are 37/17/16;
both canonical and review bundles match all 15 file-backed executable sections
of the tested binary. Evidence and focused/cumulative patches are under
`artifacts/building-studio/2026-11-13/stairwell-shared-walls/`. Work remains
uncommitted on `main`, with the actual index empty and unrelated work preserved.

## Landing guards and timber floor-opening frames · 13 November 2026

Landing perimeter guards are omitted within 0.6 m of a parallel exterior wall,
measured to its inward timber projection where present. Wider gaps retain their
guards; flight rails remain on the stringers. The same rule covers enclosed and
open stairs, both axes and reversed flights, in geometry and collision.

Floor openings have timber trimmers and headers at bearer depth. Joist ends abut
the frame's outer faces, with grain following each member. The frame is kept
inside the building footprint; cut floor edges also use timber. A shallow header
beneath the projecting landing edge completes the visible rim while preserving
headroom in the 2.4 m cottage. The remaining rim aligns with the joist soffits.

All 156 studio tests pass, including climbing and descending each flight at all
LODs, both stair axes and the compact reversed cottage. Geometry checks cover
continuous corners, joist contact and solid framing on every upper floor. Strict
all-target Clippy, crate formatting and the focused whitespace check pass. An
all-dependency formatting check also reported existing module ordering in
`island-rs/src/ffi.rs`; that unrelated file was preserved. Parallel shared-wall
test fixtures now add an atomic sequence to their temporary directory names.

Metal captures show the unobstructed outside wall and timber-framed opening from
below and on the upper floor. The signed macOS app is build 42 with geometry,
material and bake versions 38/17/16. The canonical and review apps match the tested
executable's file-backed sections. Evidence and focused/cumulative patches are
under `artifacts/building-studio/2026-11-13/stairwell-floor-frame/`. Changes remain
uncommitted on `main`, with the actual index empty and unrelated work preserved.

## Inset rim and fireplaces on every floor · 13 November 2026

The timber opening frame now projects 15 mm inward from adjoining wall planes.
The cut floor fascia follows the same inset and has a top lip into the opening;
hidden faces between adjacent floor cells are omitted. Joist ends still meet the
outer frame faces. The shallow landing header retains the tested stair headroom.

Each floor now receives a firebox, straight breast, stone-slab hearth and surround
from the shared chimney recipe/materials. Floor-relative frames borrow the recipe;
there is one roof shaft, cap and flashing set. Per-floor shaft sections stop before
the next firebox, leaving upper apertures clear. Fireplace inspection follows the
selected storey. Stair placement reserves the whole chimney wall plus the hearth
approach for the occupied rooms, including when exterior walls would otherwise
be shared with the stairwell. Explicit positions use the same rule.

All 158 studio tests pass, including 2/3/6-storey fireplaces on all four facades,
per-floor hearth geometry/collision, unobstructed firebox rays, one roof cap,
rejected stair positions on the chimney wall, inset timber corners and bearer
contact. Camera tests check fireplace targets and clear positions on each floor;
climbing/descending the compact cottage still works at every LOD. Strict all-target
Clippy, crate formatting and whitespace checks pass.

Build 43 uses geometry/material/bake versions 39/17/16. Metal captures and the
review recipe are in `artifacts/building-studio/2026-11-13/stacked-fireplaces/`,
alongside focused/cumulative diffs and bundle verification. The signed canonical
and review apps match the tested binary's file-backed sections. Changes remain
uncommitted on `main`; the actual index is empty and unrelated work is preserved.

## Outside-wall stair priority and movable entrances · 13 November 2026

Automatic placement ranks both stair axes together by the nearest usable exterior
wall, then by proximity to a corner. The stair footprint keeps a 40 mm clearance
from the inside timber face; an omitted enclosure side adds no extra setback.
The shared-chimney wall and hearth approaches remain reserved for the rooms.
Explicit stair positions and axes are respected.

For each stair candidate, entrance leaves are cached once and translated along
their existing facade while testing their full swing and player approach. Clear
authored positions are kept, including valid near-corner overrides. Obstructed
doors move to an available span before considering another stair position. The
recipe keeps authoring preferences; generated prototypes carry resolved offsets.
Generated windows are rebuilt around moved doors by moving the existing door
records into the new list. Manual windows, door IDs/styles/hinges and shared sizes
are preserved. Foundation entrance steps use the resolved door positions too.
Generation, validation, studio file opening and portable regeneration share the
same structural resolution. The editor labels multi-storey door positions as
preferred positions and explains the stair priority.

The complete 161-test studio suite passes. New checks compare five front-door
preferences in the compact cottage, prove identical stair placement at the outside
wall, retain a clear near-corner preference, preserve manual windows and panel-door
styles/hinges, verify exterior steps follow the door, and cover both axes with two
entrances. Existing climbing/descending, shared-wall thresholds, fireplaces on all
floors, upper-floor doors, and portable round trips continue to pass.

Build 44 uses geometry/material/bake versions 40/17/16. Evidence, the review recipe,
focused/cumulative patches, format and strict all-target Clippy logs and executable
section checks are under `artifacts/building-studio/2026-11-13/wall-first-stairs/`.
Changes remain uncommitted on `main`, with the actual index empty and unrelated
work preserved.

## Landing window clearance and upper-floor jambs · 14 November 2026

After stair and entrance placement, windows beside landing platforms move upward
until the full sill and casing clear the landing by 20 mm. Windows that cannot fit
below the lintel, or would overlap another opening, are omitted from the resolved
prototype at every LOD. Enclosed stairs only affect windows on reused external
walls; windows behind retained partitions are left alone. The authored recipe
retains its windows and elevations, so changing the stairs can restore them.

Upper-floor stairwell door jambs start 5 mm above the floor, removing their
coplanar bottom faces. Ground-floor jambs, door leaf elevations, hinges and opening
dimensions retain their previous positions.

All 165 studio tests pass. New checks cover raised and omitted windows on both
axes and reversed flights, complete sill clearance, unaffected windows, manual
and automatic recipe regeneration, authoring preservation, and jamb clearance
on every floor. Existing export, door swing, stair navigation and fireplace
checks also pass. Build 45 uses geometry/material/bake versions 41/17/16.
Review recipes, captures, focused and cumulative diffs, and verification logs
are under `artifacts/building-studio/2026-11-14/landing-windows/`.

## Doorway floor faces and Gothic church formula · 14 November 2026

General entrance jambs previously extended 50 mm into the floor, and the
stairwell's header beneath each upper doorway ended with an upward face exactly
at the next floor surface. All jambs now clear the floor by 20 mm; those header
caps end 20 mm beneath the boards. Regression checks inspect both trim feet and
the upward-facing wall-cap vertices at every doorway level.

The new `church` formula produces a connected cruciform nave, transepts, crossing,
chancel and two entrance towers. The footprint has a 16-vertex outline, with
non-overlapping floor/support regions and buried foundations following the cross.
Pointed lancet apertures are removed from both sides of the LOD0 wall; glass,
stone arch rings, sills, mullions and tracery complete them. Flying buttresses
retain open arches; crenellations and pinnacles finish the towers. The interior
is unfurnished, with stone floors and pointed stone ribs. The two intersecting
gable roofs share a crossing with continuous ridges and diagonal valleys.

The church is available in Starting places, `--preset church`,
`recipes/gothic-church.json` and `BuildingRecipe::gothic_church()`. Plan, tower,
buttress and shared opening dimensions are editable. The existing wall, floor,
ceiling and roof material assignments apply, with built-in stone and slate.
Interior and Window inspection target the nave and a real lancet. Exported
`layout.church` retains the outline and structural regions; portable packages
regenerate the geometry, materials, atlas and layout.

LOD1 omits the interior, uses opaque windows and reduces arch detail. LOD2 bakes
stone, slate, portal and pointed-window bays into padded lighting-free charts,
while retaining the cross, tower and open buttress silhouettes. The default
church has 49,556 / 17,560 / 1,586 static triangles. All 173 studio tests pass,
including connected collision-free routes, clear LOD0 window rays versus opaque
LOD1 rays, open buttress clearance, finite/non-degenerate geometry, invalid
proportion diagnostics and portable regeneration. Build 46 uses versions
42/18/17; evidence and review patches are under
`artifacts/building-studio/2026-11-14/gothic-church/`.

## Full-height pointed church double doors · 14 November 2026

The south entrance now has two independently hinged timber leaves filling the
complete pointed arch, replacing the glass fanlight. Their head profiles follow
the same piecewise curve as the masonry, with a 12 mm head clearance, 6 mm jamb
clearance, 8 mm centre seam and 20 mm floor clearance. Each leaf has its own knobs,
stable ID, hinge, signed swing and persisted angle. The shared door height is the
arch spring line; validation requires the complete head to fit below the wall top.
The Entrance camera frames the full arch. LOD1 has matching pointed closed leaves;
LOD2 bakes timber up to the apex, including the centre seam and assigned door map.

Optional hinge-local `collision_sections` describe shaped leaves. Rectangular
doors retain their previous collision when this field is absent or empty.
Structural swing validation, sphere/capsule/ray queries, swing blocking and debug
display all use the sections, leaving the empty corners above each curved head
clear. Door and hardware meshes remain separate, with ordinary shared materials.

All 176 studio tests pass, including full-height coverage without fanlight glass,
opposing hinges, shaped collision through the swing, independently opening either
half, saved-state restoration and portable export/regeneration of both leaves.
The final 10 focused church tests and strict all-target Clippy checks also pass.
Build 47 uses geometry/material/bake versions 43/18/18. Evidence and review patches
are under `artifacts/building-studio/2026-11-14/church-double-door/`.


## Paired Gothic lights and circular eye · 15 November 2026

Each pointed window now contains two complete pointed lights with a single
central mullion. Their heads share the outer arch's spring line; a circular eye
is tangent to both inner heads and the outer surround. The radial stone bands
have matching pointed mitres and continuous front, back and reveal surfaces.
The circular band projects 4 mm beyond the heads to avoid coplanar faces at the
intersections. LOD0 retains transparent glazing, LOD1 uses coarser tracery over
opaque glass, and LOD2 samples the same analytic stone pattern in its atlas.

Upper tower sections generate at their actual height rather than stretching a
nave-height mesh. Short sections fit the window dimensions before generating;
this keeps the eye circular and the masonry courses at their original scale.
A separate atlas chart carries the fitted tower-window proportions.

All 179 studio tests pass. Three new checks cover tangency and matching pointed
mitres, mesh/bake agreement at clear and stone probes for both geometric LODs,
and circular eyes fitting four tower-wall heights. Existing transparent/opaque
window rays, finite geometry, decreasing LOD budgets, independent double doors
and portable regeneration remain covered. Formatting and strict all-target
Clippy checks pass. Build 48 uses geometry/material/bake versions 44/18/19.
Serial Metal captures inspect the LOD0 window and nave interior, plus true
exterior LOD1/2 views. All four processes exit successfully. The signed build-48
review app regenerates the church and displays its new tracery in Window view.
The default static meshes contain 73,352 / 27,354 / 1,586 triangles; the two
interactive door assemblies are counted separately by the studio. A fresh
portable church package exports successfully with versions 44/18/19.

The prior build-47 app is preserved alongside the evidence. The canonical and
review executables have the same UUID and file-backed sections as the verified
build, and both signatures pass deep strict verification. Evidence and
focused/cumulative review patches are under
`artifacts/building-studio/2026-11-15/church-tracery/`.


## Closed pinnacle shoulders · 15 November 2026

The square stone pedestal beneath each church pinnacle now extends 25 mm past
the cone's 312 mm base radius on each axis, instead of stopping at 240 mm. Its
top overlaps the cone by 20 mm. This hides the open underside when viewed from
below and applies to buttress and tower pinnacles at every LOD, with no added
triangles. The existing cone profile and height are unchanged.

All 10 focused church/door tests, formatting and strict all-target studio Clippy
checks pass. The LOD0 Metal Window capture was inspected: the wider pedestals
cover the cone feet. Build 49 uses geometry/material/bake versions 45/18/19;
build 48 is preserved. Evidence and the focused review diff are under
`artifacts/building-studio/2026-11-15/pinnacle-base/`.


## Smaller cones on original pedestals · 15 November 2026

Restored pinnacle pedestals to their original 480 mm width and reduced the cone
base radius from 312 mm to 215 mm. The cone now fits 25 mm inside the block on
each axis, retaining the 20 mm vertical overlap to hide its open underside.
Pinnacle heights and triangle counts are unchanged at every LOD.

The 10 focused church/door tests, formatting and strict all-target studio Clippy
checks pass. The LOD0 Metal Window capture shows the slimmer cones sitting inside
the original blocks. Build 50 uses geometry/material/bake versions 46/18/19;
build 49 is preserved. Evidence and the focused diff are under
`artifacts/building-studio/2026-11-15/slimmer-pinnacles/`.


## Uniform circular flying buttresses · 15 November 2026

Replaced the elliptical buttress profile and vertical offset with concentric
circular edges 480 mm apart. The ordinary profile follows the same two-centred
pointed arch as the windows. Each curve and its support pier now span 180 mm
farther out from the nave. The wall end remains attached above the glazing;
the foot is centred over the pier, with closed undersides and end faces at every
LOD. Internal segment caps are omitted. Collision follows each new segment.
World-height masonry UVs retain level stone courses.

Short, wide recipes fit a circular segment above the floor; short spans raise the
pier enough to meet the arch foot. Two new checks cover normal thickness, agreement
with the window curve, the outward offset, and pier connections across nine
reach/height combinations. The existing 10 church/door tests pass, including
transparent window rays, open space beneath the buttresses, finite geometry,
LOD budgets and portable regeneration. Formatting and strict all-target studio
Clippy checks pass. Serial Metal captures inspect the LOD0 Window view and true
exterior LOD1/2 views; every process exits successfully.

Build 51 uses geometry/material/bake versions 47/18/19. Build 50 is preserved.
Evidence and focused/cumulative review diffs are under
`artifacts/building-studio/2026-11-15/circular-buttresses/`.


## Filled interior ceiling arches · 16 November 2026

LOD0 interior arches now extend from their curved soffits to the sloping ceiling,
using the existing arch stone material. Their top profiles follow the nave roof
and overlap its ceiling plane by 10 mm to hide the joint without coplanar faces.
The ceiling offset is shared with the roof generator. Steep roof recipes lower
the complete arch only as far as needed to retain at least 130 mm of solid web;
the pointed curve is preserved. Segment colliders cover the filled stonework,
while the passage below stays clear. Lower LODs remain exterior-only.

A new mesh-ray check verifies solid fill just below the ceiling, no protrusion
above it, a clear standing-height passage and matching collision for three
nave/roof proportions, including the steepest roof setting. It and all 10 focused
church/door checks pass, alongside formatting and strict all-target studio Clippy.
Serial Metal captures inspect the completed LOD0 interior and the exterior; both
processes exit successfully.

Build 52 uses geometry/material/bake versions 48/18/19. The previous build-51 app
is preserved. Evidence and focused/cumulative review diffs are under
`artifacts/building-studio/2026-11-16/filled-ceiling-arches/`.


## Church eaves, rough stone and exterior grub · 16 November 2026

Closed, mitred eave strips extend the nave and transept roof edges at every LOD,
including the gable ends. Slate top faces follow the roof slopes; stone fascia
and soffits close the outer edges. Structure exposes Roof overhang, with 450 mm
in new church presets and zero disabling the projection. The interior ceiling
and its filled arches retain their positions and collision.

The shared rough-stone.json recipe replaces church wall, ceiling and foundation
block courses with continuous five-octave noise. Its height field generates
normal-map relief; this does not displace mesh vertices. Interior stone and trim
also receive rough-stone maps. Floor and tread slabs retain their existing
continuous stone finish, and authored material assignments still override the
defaults. World-plane UVs keep the new stone continuous across wall cuts and
window tracery faces.

Church wall solids now put only their outward faces in the wall group; inward
faces and shared reveals use interior. New presets enable the existing broad/fine
grub shader with a 3 m floor-relative falloff. Exterior wall stone, foundations
and stairs are weathered. Interior, ceiling, floor, glass, doors and shared trim
remain clean. The mixed LOD2 atlas stays unfiltered, matching the existing
weathering policy. Portable exports include interior/trim maps and the updated
allowed-material descriptor; the base stone maps remain unfiltered.

All 185 studio tests pass. New checks cover three eave projections without moving
the ceiling or collision, joint-free noise with non-flat normal maps and clear
window apertures, and actual compiled exterior-only weathering. The final nine
church integration tests, formatting and strict all-target Clippy also pass.
Seven serial Metal captures inspect all three exterior LODs, the clean interior,
eaves and matching grub-on/off entrances. Every capture exits successfully.
The default static triangle counts are 72,360 / 27,002 / 1,842.

Build 53 uses geometry/material/bake versions 49/19/20. Build 52 is preserved.
The signed bundle matches the verified executable's UUID and file-backed sections.
Evidence, the portable export and focused/cumulative review diffs are under
`artifacts/building-studio/2026-11-16/church-stone-eaves/`.


## Supported tower corner pinnacles · 16 November 2026

Tower pinnacle posts now extend down to the base of the crenellated parapet,
with 20 mm of overlap into the tower roof slab to hide the join. Cone positions,
post widths and buttress pinnacles retain their previous dimensions. The existing
post cube is lengthened at every LOD, adding no triangles.

All 12 focused church/door tests, formatting and strict all-target studio Clippy
checks pass. The LOD0 Metal exterior capture shows the corner posts joined to
the parapet base. Build 54 uses geometry/material/bake versions 50/19/20;
build 53 is preserved. Evidence and the focused review diff are under
`artifacts/building-studio/2026-11-16/supported-tower-pinnacles/`.

## Mesh finalization and optional array batching · 16 November 2026

All 205 Studio tests pass, including source cleanup, indexed fireplace geometry,
door motion, exterior-only weathering, stair visibility, collision, 30-degree
terrain support, multi-storey routes and portable export regeneration. Formatting
and strict all-target Studio Clippy pass. New binaries launch successfully after
other graphics applications were closed; no security settings were changed.

Every assembled static LOD and each independent moving door now pass through
exact attribute-aware finalization before baking/hashing. Position, normal and
UV seams, opposing faces, material boundaries and intentional alpha multiplicity
are retained. Collapsed triangles and exact same-facing opaque duplicates are
removed. Explicit joined opaque components use exact interval clipping and
stable face ownership, preserving the original cube charts. Ground-floor
undersides and enclosed roof-ceiling tops are omitted locally, while upper-floor
undersides, window reveals, soffits and independently hideable stairs survive.

Church interior wall skins meet at shared convex/concave miters. Hidden masonry
subdivision caps are replaced by actual aperture jambs; continuous ceiling ribs
retain exposed end caps. Floor/foundation regions use joined surfaces. Six
concave-corner collider squares cover the added solid corner fill.

The eight-recipe / 24-LOD audit compares the same fixtures with build 54:

| Recipe | LOD0 triangles before | LOD0 triangles after |
| --- | ---: | ---: |
| Cottage | 3,666 | 3,450 |
| Gothic church | 72,360 | 66,346 |
| Hut | 2,618 | 2,444 |
| Wattle cottage | 53,089 | 52,873 |
| Workshop | 3,272 | 3,054 |
| Slate cottage | 4,670 | 4,454 |
| Three-storey cottage | 5,734 | 5,516 |
| Three-storey wattle | 59,333 | 59,115 |

Across all LODs, triangles fall 254,275 → 244,901 (3.69%) and vertices fall
518,013 → 432,123 (16.58%). Every moving door descriptor and leaf/hardware
triangle attribute-and-winding multiset is unchanged. Ordinary collision metadata
is unchanged. The church collision change is the intentional corner fill;
other church collision/support/stair/foundation metadata matches. Same-facing
church Wall/Interior rectangular overlap candidates fall from 36 to zero
(198 to zero pairwise square metres). These diagnostic counts do not represent
all possible hidden geometry or unique removable area.

The optional GPU compiler keeps source parts for editing/export while batching
compatible opaque surfaces with a material index constant across each triangle.
UV1.x retains building height; UV1.y selects per-material settings. Banks retain
native resolutions, physical UV scale, tangent frames and exterior-only grub.
Glass and masked straw retain their existing paths. The baked LOD2 atlas retains
its original material, UVs, texture size and clamp sampler; other far components
reuse the same banks to avoid duplicated foundation/stair maps. Stairs and
moving leaves remain independent; hardware can share a leaf mesh. Unused bank
layers and replaced image assets are released before upload.

Serial Metal captures cover church exteriors/interiors/corners/window reveals,
wattle interiors and roof undersides, upper-storey interiors, LOD1/2, open double
doors, internal stairs, exterior stairs on a 30-degree slope, hidden stairs, and
SSAO with normal/depth/motion prepasses. Standard/array images match visually;
several pairs, including all three stair views, are byte-identical. The initial
church exterior comparison has mean absolute RGB difference 0.00119/255. Door
hardware attributes and hinge-relative heights are verified by mesh/compiler
checks; knobs are occluded in the fully-open frontal church capture.

Performance was tested with the debug Studio build (optimized dependencies) on
Apple M4 Pro / 14 CPU cores / 24 GiB, Metal, at 1600×1100. Seven scenarios cover
1/32/128 repeated cottages, 128 cottages with eight variants at LOD0/1, 32 churches
with four variants, and 32 wattle cottages with four variants. Each uses ABBA
order, 300 warm frames after pipelines are ready, and 600 measured frames per
run: 28 runs / 16,800 measured frames. Each frame waits for submitted GPU work
to complete. These are synchronized offscreen frame intervals including CPU
work, not isolated GPU timestamps or presented game FPS. CPU pass spans are
reported separately; nested spans are not added together.

| Scenario | Standard median ms | Arrays median ms | Assessment |
| --- | ---: | ---: | --- |
| 32 churches / 4 variants / LOD0 | 8.4233 | 8.0600 | Both pairs improve; about 4.31% lower |
| 128 cottages / 8 variants / LOD1 | 6.2057 | 5.8472 | Both pairs improve; about 5.78% lower |
| 128 cottages / 8 variants / LOD0 | 7.1254 | 7.1350 | Essentially unchanged |
| Repeated cottages / LOD0 | varies | varies | Larger cases have conflicting paired outcomes |
| 32 wattle cottages / 4 variants / LOD0 | 10.4269 | 9.5647 | Aggregate hides one improvement and one regression |

Table values average the two run-level medians, not pooled-frame medians.
Two runs per mode provide directional evidence, not statistical significance.
Static compiled sections fall from 13 to 3 for a cottage, 10 to 3 for a church,
and 19 to 9 for this wattle recipe; a cottage leaf/hardware pair falls from two
sections to one. These are mesh sections, not measured GPU submissions: Bevy
already instances repeated placements. CPU image payload after warmup is equal
for both compilers in every compared case, including far-bank reuse. Payload
bytes exclude driver allocations and are not a VRAM measurement.

**Decision: retain Standard as the default; arrays remain opt-in.** The benefits
are workload-dependent and do not justify a global default change. Use the
experimental inspection checkbox or `--renderer arrays` for measured workloads;
`--renderer standard` retains the original path. The geometry cleanup is enabled
for both renderers.

The packaged GUI was also checked with live array switching and roof/ceiling
cutaway, then restored to Standard with the full roof. The bundle signature,
Mach-O UUID and all 15 file-backed sections match the verified executable.

Geometry/material/bake identities are 51/19/20. Build 55 packages this source;
build 54 is preserved under `mesh-optimisation/previous-build-54/`. Detailed audit,
28 performance reports, independent assessment, captures, source manifests,
signature/section verification and focused review diff are under
`artifacts/building-studio/2026-11-16/mesh-optimisation/`.


## Church opening reveal ownership · 16 November 2026

Confirmed a second layer of masonry jamb/head faces directly coincident with
the stone surrounds. The surround now owns the lined boundary at LOD0. Masonry
retains the exposed 20 mm strip below each raised door frame. Arch masonry
omits its covered underside instead of using a depth offset or moving the frame.

The regression test fails on build 55 with both Interior and Trim at a jamb;
it now finds one Trim surface across both jambs and all 24 head segments, on
four wall orientations and three thicknesses. It also verifies the exposed
door-frame base. All 206 Studio tests, formatting and strict all-target Clippy
pass. Eight Metal close-ups/exterior/interior captures cover Standard and arrays.

Church static LOD0 falls 66,346 → 64,218 triangles (2,128 removed). The export
comparison confirms only Interior geometry changes: 2,132 triangles removed and
four shorter door-base triangles added. All other material triangle attributes,
all moving door/hardware geometry, collision/support/layout metadata and LOD1/2
geometry match build 55 exactly. Texture maps, glass and the lining stay in place.

Build 56 uses geometry/material/bake identities 52/19/20 and preserves build 55.
The signed app matches the verified executable's UUID/file-backed sections.
Evidence, the failing/passing regression, exported comparison, close-ups and
focused source diff are under
`artifacts/building-studio/2026-11-16/church-opening-faces/`.


## Shared procedural stone defaults · 16 November 2026

Foundations, stair bodies, chimneys and fireplace breasts now use the same
continuous `rough-stone.json` recipe as the church. Layered noise supplies the
colour variation and normal-map relief; the 2 × 2 m physical repeat remains.
Their independent palette colours and JSON overrides are retained. The editor
labels and bundled recipe set now match these defaults. Tread/landing slabs and
the hearth retain their separate joint-free dressed stone; weathering eligibility
is unchanged so shared interior/exterior chimney geometry remains clean.

All 206 Studio tests pass, including material recolouring, custom dependency
refresh, portable export, Save As and UV module checks. The continuous stone
uses a full texture-repeat module in the UV editor instead of a masonry course.

The cottage export retains byte-identical meshes at every LOD, moving doors,
collision/support, layout, foundation and stair metadata, and all non-stone
material maps including the slab maps. Only foundation/chimney maps, the distant
atlas and prototype identity change. All 41 package files pass length/SHA-256
verification. Seven Metal captures cover Standard, arrays, foundation views,
interiors/fireplace and the baked LOD2 exterior, with no shader/runtime errors.
Formatting and strict all-target Clippy pass.

Build 57 advances material/bake identities to 20/21 and retains geometry 52.
The prior build 56 is preserved. Evidence and the focused source diff are under
`artifacts/building-studio/2026-11-16/procedural-stone/`.


## Configurable rectangular window panes · 17 November 2026

Building-wide window pane rows/columns now default to 2 × 2. Manual windows and
stable generated-opening overrides can independently select their own layout;
1 × 1 has no internal glazing bars, and the Shop grid shortcut uses 8 × 4.
Both axes accept 1–32. The recessed sash uses solid, non-overlapping bar runs;
LOD1 retains their layout as flat exterior joinery over opaque glazing and LOD2
bakes the same grid. Gothic tracery retains its existing geometry. The bundled
`recipes/shopfront.json` places a 5.2 × 2.1 m, 32-pane front window beside the door.

All 212 Studio tests, formatting and strict all-target Clippy pass. New checks
cover legacy JSON, independent pane/dimension inheritance, generated IDs across
resizes, override reset, all four facades at LOD0/1, clear pane centres, actual
divider triangles, single-axis grids, invalid/inactive overrides and a 32 × 32
grid on a minimum-size opening. Pane edits change joinery, the atlas and identity
while preserving door geometry, collision/support, LOD2 meshes and material maps.
Round-tripped recipes regenerate the same identity.

Metal captures compare one large clear pane with 32 panes in both renderers.
Exterior HUD captures confirm actual LOD1 (2,496 triangles) and LOD2 (158), with
opaque glazing and the pane grid retained. Close window inspection correctly
pins LOD0, so those initial captures do not establish lower-LOD rendering.
No shader/runtime errors were found.

Build 58 uses geometry/material/bake identities 53/20/22 and preserves build 57.
Focused source snapshots, captures and validation logs are under
`artifacts/building-studio/2026-11-17/window-panes/`.


## Lean-tos and gabled wings · 17 November 2026

The previously reviewed building/material work was committed and pushed to
`origin/main` as `cd10794` before this extension work began. The new recipe
`extensions` list supports up to eight ground-floor rooms on rectangular
buildings, including multi-storey parents. Each attachment has a stable ID,
facade, offset, width, outward depth, eaves height, roof rise, optional generated
windows and optional pane layout. The church keeps its separate cruciform
formula. `recipes/extensions-cottage.json` demonstrates a thatched lean-to and
gabled wing together; Studio exposes the same data in **Lean-tos / wings**.

Components share material textures and retain one selected-resolution distant
atlas. Actual generated roof triangles drive the roof intersections, with a BVH
limiting comparisons to nearby surfaces. Rear roof transitions use an affine
shear so separately triangulated slate courses, thatch bodies and ridge caps
retain their contacts. Attached single-pitch thatch omits its high-side ridge
cap. Occupied rooms open the shared walls; loft infill above their ceilings
remains closed behind the roof dressing. The floor subtraction uses complete
slab footprints, rather than inner room bounds, to eliminate coincident floor
faces through the former wall thickness. Covered foundation tops are removed.

Seven integration tests cover both attachment types on all four facades,
walkable capsule clearance, visible passage clearance and retained loft infill,
one floor surface through each connection, floor support, clear LOD0 glazing,
opaque lower LODs, finite geometry, shared material maps/atlas budget,
deterministic regeneration, timber/slate/thatch and multi-storey parents,
validation failures, compound terrain depth/occupancy, and portable export.
A dense ray grid checks continuous slate roof coverage at all three LODs. It
caught an exact-boundary clipping error that discarded fragments when an edge
passed through an existing vertex; boundary vertices now belong to both
adjoining fragments while wholly coplanar faces retain deterministic ownership.

Foundations keep the actual compound footprint, sharing burial depth calculated
from its enclosing diagonal at 30°. Entrance stairs use those complete placement
bounds. The sloped terrain preview touches the highest point of the actual
foundation union, including off-centre extensions. Exported layout includes
extension rooms, connection bounds and window centres; occupancy excludes empty
courtyard corners. The portable-export test verifies regeneration of geometry,
collision, layout, atlas and prototype identity.

Build 59 uses geometry/material/bake identities **54/20/23**. Build 58 is
preserved under the evidence directory. The signed canonical bundle's UUID and
all 15 file-backed Mach-O sections match the freshly built executable, and the
bundled example and README match the source. The example has **104,955 / 6,050 /
662** static triangles, excluding retained moving doors and hardware.
Evidence, regression logs, captures, package checks and the focused diff are
under `artifacts/building-studio/2026-11-17/extensions/`.

The final full Studio/all-target suite passes **219 tests**, with strict
all-target Clippy, formatting and whitespace checks also passing. Seven serial
Metal captures cover both roof junctions, the connected interior, Standard and
array rendering, actual LOD1/2 HUD states and 30° terrain. No shader/runtime
errors were found. The signed build-59 review app loads the example, switches a
lean-to to a gabled wing and regenerates successfully. Undo restores the clean
recipe and original preview; the Extension inspection view shows the connected
room with clear glazing. The live review ends in Overview with material units
restored, leaving the example unchanged on disk. Numeric dimension editing was
not established through the computer-use harness; the live acceptance here is
roof-type switching, Undo and interior inspection.

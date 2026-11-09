# Building Studio validation — 10 November 2026

Implementation and checks were performed in the main Island-experiment checkout
on branch `main`. Changes remain uncommitted. Existing Unity weather/wetness work
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

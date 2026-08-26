# Bevy procedural material studio plan

## Status

Implemented on 26 August 2026 as the standalone `island-material-studio`
crate. The implementation retains the existing recipe format, generated
texture bytes, Unity editor, and Bevy island viewer contracts. This document
now records the architecture and acceptance criteria used for the build.

Unrelated Unity assets that were already present in the worktree remained out
of scope and were not included in the studio commits.

## Desired outcome

Create a native desktop application that can open, edit, preview, validate,
save, and bake the same JSON recipes used by `island-rs` and Unity, without
requiring Unity or launching the command-line baker as a child process.

The first complete version should provide:

- New, Open, Save, Save As, Revert, Validate, Preview, and Bake.
- Complete typed editing of the base material, global output settings, and the
  ordered layer stack.
- Add, duplicate, rename, enable, reorder, and delete layer operations.
- Undo and redo for field edits and layer operations.
- Albedo, height, normal, occlusion, packed-mask, and selected-layer 2D views.
- Single-tile and 2x2 seam views with zoom, pan, and pixel inspection.
- A lit Bevy preview on a sphere and plane using parallax-mapped height.
- Debounced background preview generation with stale-result rejection and a
  bounded cache.
- Validated atomic recipe saves with external-change detection.
- Transactional final baking through the existing Rust output writer.
- The same deterministic evaluator and bytes as the existing CLI at matching
  recipe settings.

## Architectural decision

Add a new sibling crate:

```text
Island-experiment/
  island-rs/                  engine-neutral recipe/evaluator/output library
  island-bevy/                existing island viewer
  island-material-studio/     new standalone Bevy authoring application
```

`island-material-studio` should depend on `island-rs` by path and use the same
Bevy 0.19 and `bevy_egui` 0.42 lines already used by `island-bevy`.

Do not put Bevy dependencies into `island-rs`, and do not fold the studio into
the island viewer. The evaluator remains usable without a renderer, while each
application keeps a focused binary, dependency set, settings file, and window
lifecycle.

The studio should call `island-rs` directly in process:

```text
egui controls
    -> typed TextureRecipe owned by the document
    -> validate_recipe / generate_preview on a background task
    -> typed CPU images
    -> one shared Bevy image-conversion path
    -> 2D egui views and the lit Bevy material

explicit Bake
    -> generate_texture_set
    -> write_texture_set
    -> completion manifest and generated PNGs
```

There must be no second noise implementation, GPU approximation, or Bevy-only
recipe format.

## Ownership model

The UI thread owns one `StudioDocument`, and that document owns one current
`TextureRecipe`. Controls mutate it only through an edit transaction so dirty
state, validation, undo history, and preview revision advance together.

Snapshots deliberately clone the recipe at transaction boundaries. Recipes
are bounded to 64 layers and are small compared with their generated images;
this makes undo reliable and keeps field-level inverse operations out of the
model. Slider dragging creates one snapshot when the drag begins and commits
one transaction when it ends, rather than cloning on every frame.

A preview or bake task receives one owned recipe clone because a Bevy compute
task must be independent of the mutable document. The task borrows that owned
recipe while calling `island-rs` and returns one owned result. The scheduler
allows one running request and retains only the newest pending request. A
revision number causes late results to be discarded. If cooperative
cancellation is added, the only shared ownership should be a small
`Arc<AtomicBool>` cancellation flag; image buffers should never be shared by a
mutex.

Bevy's asset stores own uploaded GPU images and materials. The preview cache
owns the corresponding CPU result only while it is needed for pixel inspection
or reuse. Cache entries are capped initially at eight complete previews.

## Crate and module layout

Suggested new files:

```text
island-material-studio/
  Cargo.toml
  README.md
  src/
    main.rs                 command-line parsing and App construction
    app.rs                  plugin composition and top-level state
    document.rs             open document, dirty state, revisions
    history.rs              bounded edit transactions, undo, redo
    file_io.rs              dialogs, external-change checks, atomic save
    preview.rs              request/result types and scheduler systems
    preview_images.rs       typed island-rs image -> Bevy Image conversion
    preview_scene.rs        render target, plane/sphere, camera and lighting
    bake.rs                 background bake and completion reporting
    settings.rs             persisted UI preferences and recent files
    ui.rs                   UI module root and panel composition
    ui/
      toolbar.rs
      layer_stack.rs
      inspector.rs
      preview.rs
      diagnostics.rs
```

Use file-named module roots throughout the crate: `ui.rs` declares the
submodules stored under `ui/`. Do not create any `mod.rs` files.

Small reusable engine-neutral preview functionality should move from
`src/bin/texture_baker.rs` into:

```text
island-rs/src/procedural_textures/preview.rs
```

That module should own typed preview settings/results and selected-layer map
construction. The CLI should become a file-protocol adapter over that API, and
the Bevy app should consume the typed images directly. PNG preview encoding and
manifest output remain CLI concerns.

## Rust library boundary

Add a narrow preview API rather than exposing Bevy types or editor state from
`island-rs`:

```rust
pub struct PreviewSettings {
    pub selected_layer_id: Option<String>,
}

pub struct PreviewMaps {
    pub textures: TextureSet,
    pub packed_mask: Option<Rgba8Image>,
    pub selected_layer: Option<LayerPreviewMaps>,
    pub recipe_hash: String,
    pub timings_ms: PreviewTimings,
}

pub struct LayerPreviewMaps {
    pub raw: FloatImage,
    pub remapped: FloatImage,
    pub mask: FloatImage,
}

pub fn generate_preview(
    effective_recipe: &TextureRecipe,
    settings: &PreviewSettings,
) -> Result<PreviewMaps, TextureError>;
```

The scheduler should create its owned effective recipe once, override its
width and height to 128, 256, or 512, and then borrow it for generation. This
avoids another recipe clone inside the library.

Also add borrowed metadata lookup such as `property_metadata(pointer: &str)` so
the typed Rust UI can reuse Rust-owned labels and tooltips without serializing
and reparsing the JSON schema. Keep `schema_document()` unchanged for Unity and
other non-Rust clients. Variant editors should use exhaustive `match` blocks so
a newly added recipe enum variant creates a compile-time task in the studio.

If a valid New document cannot be constructed cleanly from the current public
API, add a tested `Default` implementation or a `default_texture_recipe()`
factory in `island-rs`. Do not embed a second default JSON recipe in the app.

## User interface

Use ordinary egui panels instead of adding a docking framework in the first
version:

```text
+----------------------------------------------------------------------------+
| New Open Save Save As Revert | Undo Redo | Auto Preview | Validate | Bake  |
+----------------------+-------------------------------+---------------------+
| Base / Layers        | Selected inspector            | Preview             |
|                      |                               | A H N AO Mask Layer |
| Base material        | Source                        | Lit                 |
| Layer stack          | Remap                         |                     |
| [x] Broad detail H A | Mask                          | [image or 3D view]  |
| [x] Fine grain    H  | Height output                 |                     |
| [x] Colour wash   A  | Albedo output                 |                     |
|                      |                               |                     |
| + Add Dup Delete     |                               |                     |
+----------------------+-------------------------------+---------------------+
| validation, preview timing, bake progress, document path and dirty state   |
+----------------------------------------------------------------------------+
```

The left panel owns selection and order. The middle inspector switches between
base settings and the selected layer, and shows only fields relevant to the
active tagged enum variants. The right panel owns map tabs, single/2x2 tiling,
zoom/pan, preview resolution, auto-preview, and lit controls.

Keyboard shortcuts:

- Command/Ctrl+N, O, S, and Shift+S for document operations.
- Command/Ctrl+Z and Shift+Z for undo/redo.
- F5 or Command/Ctrl+Enter for manual preview.
- Delete for the selected layer only when a text field does not own input.
- Space-drag or middle-drag to pan a 2D view; wheel to zoom.

Every layer row should expose enabled state, editable name, H/A routing badges,
and invalid-mask state. Diagnostics should retain JSON Pointer paths so clicking
one selects the layer and focuses or highlights the relevant field.

## Document safety

`StudioDocument` should retain:

- The current typed `TextureRecipe`.
- Source path, source-byte hash, and last successfully saved canonical form.
- Dirty state, selected layer ID, and monotonic edit revision.
- A bounded undo/redo history, initially 100 transactions.
- The last successful bake recipe hash.

Opening parses with Serde and validates with `validate_recipe`. Saving first
validates the current recipe, writes pretty JSON plus a trailing newline to a
temporary sibling, and atomically replaces the target. Immediately before an
existing file is replaced, compare its current byte hash with the hash captured
at open or last save. On mismatch, offer Reload, Save As, or explicit
Overwrite.

New, Open, Revert, closing the window, and quitting the application must all
protect a dirty document with a Save / Discard / Cancel modal. Native file
dialogs may use a small cross-platform dialog dependency, but their blocking
work must not freeze an active generation frame.

A later hardening phase may add a recovery snapshot under the platform's user
data directory. Recovery must be identified by source path plus content hash
and must never overwrite a recipe automatically.

## Preview generation

The preview scheduler should use Bevy's async compute task pool and follow this
state machine:

```text
edit committed
    -> validate cheaply on the UI thread
    -> mark revision dirty
    -> debounce for 300 ms
    -> replace the pending request with the newest recipe snapshot
    -> run at most one preview task
    -> accept only a result matching the current revision and settings
    -> upload all maps together in one Bevy system
```

Invalid recipes should show diagnostics and keep the last valid preview on
screen. A failed or stale result must never partially replace the displayed map
set. Manual Preview bypasses the debounce but not validation.

Cache keys should include the normalized effective recipe hash, preview
dimensions, selected layer ID, and the layer diagnostic mode. Cache hits should
reuse a complete coherent map set. Cache eviction must remove the associated
egui registrations and Bevy asset handles so GPU memory stays bounded.

Start with latest-result rejection and a one-running/one-pending queue. If a
512-pixel preview can keep the worker busy long enough to harm interaction, add
cooperative cancellation checks at natural evaluator boundaries and row-block
boundaries, with focused determinism tests proving that cancellation does not
change completed output.

## 2D preview

Create all Bevy images through one conversion module with an explicit map
contract:

- Albedo: RGBA8 sRGB.
- Normal: RGBA8 linear, with Bevy's `flip_normal_map_y` derived from the recipe
  convention in the lit material rather than rewriting pixels.
- Occlusion: linear grayscale expanded into the channel layout Bevy's standard
  material expects.
- Height: retain R16/float CPU data, plus a display RGBA8 grayscale image using
  fixed-range or auto-levelled presentation.
- Packed mask: RGBA8 linear with a selectable all/R/G/B/A display.
- Layer raw/remapped/mask: display RGBA8 grayscale generated from the typed
  diagnostic arrays.

The same row-major orientation and UV convention must feed every map. Add an
asymmetric corner-marker test that proves albedo, height, normal, AO, mask, and
layer maps all address the same source pixel. This directly guards the map
misalignment class already encountered in the Unity lit preview.

The egui viewer should support nearest and linear filtering, pixel coordinates
and values, physical tile dimensions, one tile or 2x2 repeat, fit-to-panel, and
reset zoom.

## Lit preview

Render the lit preview to an off-screen Bevy image and register that image with
egui. Keep the preview world deliberately small:

- One UV sphere and one plane, with only one visible.
- One perspective camera with orbit and zoom.
- Directional light plus adjustable ambient light.
- Neutral background and predictable exposure.
- A `StandardMaterial` using albedo, normal, occlusion, and Bevy parallax depth.

Expose map toggles, plane/sphere selection, tiling, roughness, light azimuth,
light elevation, intensity, ambient strength, height scale, and reset view.
Normal convention should map to `StandardMaterial::flip_normal_map_y`; no map
should be flipped independently in an ad hoc upload path.

Bevy's depth map treats white as the bottom and black as the top, so derive one
linear depth texture as `1 - normalized_height` without changing row order or
UV orientation. Set `parallax_depth_scale` from the recipe's physical
displacement range relative to its tile size, multiplied by the artist's
height-scale control. Test this conversion explicitly. Parallax is the only 3D
height technique in the studio; the preview does not promise displaced
silhouettes or geometry.

The 2D and lit views must receive one atomic `PreviewAssets` replacement so a
new albedo can never be displayed with an older normal, AO, or height map.

## Final bake

Final baking is explicit and runs on a background compute task using the
document's real width and height. The bake panel selects an output directory,
output profile, and whether an existing generated set may be replaced.

Call `generate_texture_set` and `write_texture_set` directly. Preserve the
writer's existing containment, known-file replacement, rollback, manifest, and
hash safeguards. Show per-stage timings and the final manifest path. Update the
document's last-bake hash only after the output writer reports success.

Unlike the Unity editor, the standalone app does not import engine assets or
assign a Unity material. `motu_unity_terrain` remains available as an output
profile for users who want its packed mask.

## Implementation phases

### Phase 0: lock the current baseline

- Record the exact dirty-worktree state without staging or cleaning it.
- Build the release baker and capture current 128x128 hashes for both committed
  recipes from the current evaluator.
- Capture current full-size manifest hashes for reference without rewriting
  generated assets.
- Run the focused procedural-material tests, baker CLI tests, format, and
  Clippy; record the unrelated river and Unity warnings separately.

Exit gate: a reproducible baseline exists before shared preview code moves.

### Phase 1: make preview generation a library API

- Add `procedural_textures::preview` with typed settings, maps, layer
  diagnostics, hash, and timings.
- Move shared preview-map construction out of `texture_baker.rs`.
- Adapt the existing CLI preview command to the new API without changing its
  JSON envelope, filenames, PNG bytes, cleanup rules, or manifest.
- Add metadata lookup and a valid recipe factory if needed.

Exit gate: CLI preview outputs and locked recipe hashes are unchanged, and the
new API is covered without Bevy.

### Phase 2: application shell and document lifecycle

- Create `island-material-studio` with Bevy, `bevy_egui`, and the path
  dependency on `island-rs`.
- Add the fixed three-panel layout, command-line recipe path, native file
  dialogs, recent-file settings, dirty-close modal, and status bar.
- Implement typed open/new/revert/save/save-as and external-change detection.
- Implement bounded edit transactions and undo/redo.

Exit gate: both recipes round-trip without semantic change, external edits are
  never silently overwritten, and all destructive document actions protect
  dirty work.

### Phase 3: complete typed recipe and layer editing

- Add base material editors for layered noise, cracked stone, and rounded
  stones.
- Add displacement, normal, occlusion, albedo, and output-profile editors.
- Add the layer stack and editors for source, warp, remap/curve, mask, height
  output, albedo output, colour ramp, and gradient stops.
- Add add/duplicate/reorder/enable/rename/delete operations and stable unique
  layer IDs.
- Connect Rust diagnostics to layers and controls.

Exit gate: no JSON hand-editing is required for any field in either committed
recipe or any currently supported tagged enum variant.

### Phase 4: asynchronous 2D preview

- Add debounce, revisioning, one-running/one-pending scheduling, errors, and
  the eight-entry cache.
- Upload coherent Bevy/egui images with the shared orientation contract.
- Add all map tabs, selected-layer diagnostics, 2x2 tiling, zoom, pan, and
  pixel inspection.
- Keep the last valid preview visible while a new one runs or validation fails.

Exit gate: rapid edits cannot display stale or mixed map sets, preview work
  does not block UI interaction, and a 256x256 release preview normally lands
  within one second on the development machine.

### Phase 5: lit Bevy preview

- Add the render-to-texture preview scene, plane/sphere switching, orbit/zoom,
  light controls, and material map toggles.
- Add parallax height using Bevy's standard material depth map.
- Verify normal convention, UV orientation, linear/sRGB formats, and atomic map
  replacement with an asymmetric reference material.

Exit gate: lit features align with the 2D maps, seams remain continuous in 2x2
  mode, and every map toggle has an obvious isolated effect.

### Phase 6: transactional baking and hardening

- Add background full-resolution bake, output profile selection, overwrite
  confirmation, progress/timings, and manifest reporting.
- Add optional cooperative cancellation if profiling justifies it.
- Add recovery snapshots, settings persistence, accessibility labels, keyboard
  traversal, and clear failure modals.
- Add README instructions and root-project links.

Exit gate: the standalone app can perform the entire authoring workflow safely
  without Unity, while the existing CLI and Unity studio continue to work.

### Phase 7: packaging

- Produce an optimized application bundle for macOS first, then verify Windows
  and Linux build requirements if those platforms are wanted.
- Keep development launch available as:

  ```sh
  cargo run --release \
    --manifest-path island-material-studio/Cargo.toml -- \
    --recipe island-rs/texture-recipes/rounded-river-stones.json
  ```

- Document that packaging validation is separate from a local Cargo run.

Exit gate: the packaged binary starts without Cargo or Unity installed and can
  open, preview, save, and bake a recipe in a user-writable location.

## Automated validation

### `island-rs`

- The two committed recipes still load and preserve their locked outputs.
- Library preview at a chosen resolution matches final generation at that
  resolution.
- The CLI adapter preserves response schema, filenames, hashes, cleanup, and
  failure behavior.
- Every selected-layer raw/remapped/mask map has the expected dimensions and
  stable layer ID.
- The asymmetric map-orientation fixture addresses the same corner in every
  map.
- Metadata lookup covers every property used by the studio.

### `island-material-studio`

Keep document, history, scheduling, cache-key, and file-safety logic in ordinary
Rust types so most tests do not require a GPU or a window.

- Open and untouched save round-trip both recipes.
- Invalid JSON and invalid recipes leave the current document intact.
- Dirty state returns to clean when undo reaches the saved content.
- Slider edits coalesce into one undo transaction.
- Add, duplicate, reorder, rename, enable, and delete preserve valid layer IDs
  and repair selection predictably.
- Earlier-layer mask references remain valid or surface a precise diagnostic
  after reordering/deletion.
- Stale preview results are rejected and the newest pending request wins.
- Cache keys separate resolution, recipe, selected layer, and preview mode.
- External file changes block ordinary Save.
- Atomic save failure leaves the previous file readable and cleans its temp
  file.
- Bake success requires a complete manifest; failure does not update the
  last-bake hash.

Run format, locked tests, and strict Clippy independently for `island-rs`,
`island-bevy`, and `island-material-studio`. Do not describe the repository as
fully green if the existing unrelated river-continuity test still fails.

## Manual acceptance

Test a release build at 1440x900 and at a compact 1100x700 window:

1. Open each committed recipe and compare all 2D maps with a same-size CLI
   preview.
2. Edit every base-material section and confirm the correct controls and
   preview response.
3. Build a three-layer recipe with height-only, albedo-only, and combined
   layers.
4. Exercise every source, blend, mask, colour-map, and material variant.
5. Reorder and delete referenced layers while previews are running.
6. Exercise undo/redo across field, curve, gradient, and layer operations.
7. Inspect one tile and 2x2 seams for every map.
8. Compare the lit plane and sphere while toggling albedo, normal, AO, and
   height independently.
9. Save, externally modify, and confirm Reload / Save As / Overwrite behavior.
10. Bake both output profiles, inspect the manifests, and compare generated
    hashes with the CLI.
11. Close or quit with dirty edits and exercise Save, Discard, and Cancel.
12. Run the packaged application without Unity or a Cargo invocation.

## Risks and controls

| Risk | Control |
| --- | --- |
| Bevy app and CLI previews drift | One typed preview API in `island-rs`; the CLI is only an encoder/protocol adapter |
| UI blocks during CPU generation | Background task, debounce, one running request, one newest pending request |
| Old tasks replace current maps | Revision IDs and atomic whole-set replacement |
| Map rows or conventions diverge | One conversion module, explicit formats, corner-marker tests, Bevy normal-Y flag |
| Undo consumes excessive memory | Recipe-only snapshots, bounded history, one transaction per gesture |
| Preview cache consumes GPU memory | Eight-entry LRU and explicit Bevy/egui handle removal on eviction |
| Schema changes leave missing controls | Typed exhaustive enum matches plus shared metadata coverage tests |
| Save overwrites outside edits | Source byte hash and explicit conflict choices before atomic replacement |
| Final bake partially replaces a set | Existing transactional `write_texture_set` contract and completion manifest |
| Adding the studio bloats the core library | Separate Bevy application crate; `island-rs` remains engine-neutral |

## Non-goals for the first release

- An unrestricted node graph or cyclic layer dependencies.
- A GPU reimplementation of procedural sampling.
- Editing Unity materials or importing assets into a Unity project.
- Combining the authoring studio with the 3D island viewer.
- Multiple documents in tabs.
- Custom shader authoring.
- Network collaboration or cloud recipe storage.

## Completion criteria

The standalone studio is complete when:

- Both committed recipes open, edit, save, and bake without Unity.
- Every current recipe field and layer operation is available through the UI.
- Preview and final bake call the same `island-rs` evaluator.
- 2D, tiled, selected-layer, and lit previews are coherent and correctly
  aligned.
- Previewing remains responsive and stale results cannot appear.
- Undo/redo, dirty-close protection, atomic save, and external-change detection
  protect authoring work.
- Full baking is deterministic and transactional.
- Focused Rust tests, format, strict Clippy, headless application logic tests,
  and manual release visual acceptance pass.
- The existing Unity studio and CLI retain their current contracts.

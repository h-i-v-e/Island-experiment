# Unity procedural material editor plan

## Status

Implementation and editor-specific validation completed on 2026-08-25. The
generator remains engine-neutral and deterministic; Unity edits the Rust-owned
JSON format and does not contain a second texture-generation implementation.

Delivered and verified:

- The current layer format, committed recipes, schema metadata, JSON Pointer
  diagnostics, validation and preview protocol are implemented in Rust.
- Locked 128x128 hashes protect all five maps for both recipes, and focused
  tests cover every source, height blend, albedo blend, routing combination,
  preview/final parity and JSON failure envelope.
- Procedural Material Studio provides schema-driven editing, file operations,
  Undo/Redo, layer operations, diagnostic focusing, context reset/copy/paste,
  asynchronous cancellation, cached previews, timing telemetry, tiled/panned
  pixel inspection, mask channels and the lit plane/sphere view.
- Save, bake, import and material assignment retain their validation,
  path-containment, completion-manifest and all-required-map safeguards.
- Rust formatting, 254 library tests, six baker tests, three editor integration
  tests and strict Clippy pass. Unity 6000.5.6f1 compiles the editor and its
  release-baker integration validation passes both committed recipes.
- A live Unity editor launch opened the studio and completed a 256x256
  auto-preview with nine declared maps outside `Assets`; the generated albedo
  and normal maps were visually inspected.

Two repository-wide Rust diagnostics remain outside this feature: the existing
river-continuity integration test and one seam-diagnostic corner-profile test.
Unity also logs the repository's existing `TagManager.asset` parser warning;
none is caused by or suppresses Procedural Material Studio validation.

The working name for the editor is **Procedural Material Studio**.

## Desired outcome

An artist should be able to open either existing recipe, reproduce its current
textures, alter the base material and ordered noise layers, decide how every
layer contributes to height and/or albedo, inspect the result immediately, and
save or bake the same portable JSON recipe used by other game engines.

The completed tool should provide:

- New, Open, Save, Save As, Duplicate, Revert, Validate, Preview and Bake.
- Complete editing of both existing recipes after they are updated to the new
  layer format.
- An ordered, reorderable stack of noise layers.
- Value, fBM, billow, ridged and cellular noise sources.
- Domain warp, frequency, octaves, lacunarity, gain, offset, seed and cellular
  controls where relevant.
- Independent height and albedo influence for each layer.
- Blend operations, strength, remapping and optional masks.
- Fast 2D map previews and a lit 3D material preview.
- Deterministic output identical to command-line baking.
- One current Rust-owned recipe format shared by Unity and other engines.

## Current baseline

The existing implementation already provides a strong foundation:

- Rust owns `TextureRecipe`, validation, periodic sampling, field evaluation,
  normal generation, occlusion, albedo, packing and PNG output.
- The current recipe has three base material models: layered noise, cracked
  stone and rounded stones.
- `surface_layers` already supports value, fBM, billow, ridged, cellular
  distance, cellular edge, cellular value and recursively domain-warped noise.
- Height layers are evaluated in order and support replace, add, subtract,
  multiply, minimum, maximum, fixed lerp and noise-masked lerp.
- The existing Unity window invokes the baker synchronously, imports completed
  textures and optionally assigns them to the terrain material.

The important limitations are:

- Unity cannot edit recipe contents; it only chooses a JSON file.
- The window blocks while the Rust process runs.
- Surface layers currently affect only the authoritative height field.
- Albedo is controlled by one material-wide palette and response block.
- There is no layer isolation, live preview or undo/redo.
- Hand-writing matching C# recipe classes would make Rust and Unity drift apart.

## Architectural decisions

### Rust remains the source of truth

The JSON schema, defaults, validation and evaluation rules remain in
`island-rs`. Unity edits a JSON document and asks Rust to validate and render
it. No noise formula should be reimplemented in C# or a Unity preview shader.

This keeps saved recipes usable from Unity, Bevy, Godot, Unreal integrations or
standalone asset pipelines.

### Use an ordered layer stack, not an unrestricted node graph

An ordered stack directly represents the current evaluation model and is much
easier to understand, reorder, validate and reproduce. Each layer produces one
scalar field and routes it independently to height and albedo.

A layer may use an earlier layer as a mask. Forward references and cycles are
rejected. This provides useful inter-layer interaction without requiring a
general graph editor in the first release.

### Preview and final bake use the same evaluator

Preview generation may override image dimensions and request diagnostic maps,
but it must call the same Rust field, albedo, normal and occlusion code as the
final bake. A GPU approximation would be faster but could mislead the artist.

### Preview files do not enter the Asset database

Temporary recipes and previews live under
`Library/ProceduralMaterialPreview/`. Only an explicit Bake writes under
`Assets/Generated/Textures` and triggers texture importing or material
assignment.

### Keep one current document format

Unity should edit the recipe through a JSON document model rather than fully
deserialising and reserialising it through duplicated C# data-transfer classes.
Known values are patched by JSON Pointer and the whole current-format document
is validated by Rust before saving or previewing.

Use Unity's official, pinned `com.unity.nuget.newtonsoft-json` package for this
document model. `JsonUtility` is not suitable for tagged enums, recursive noise
sources or schema-driven editing.

## Replace the recipe format in place

There is no deployed recipe format to preserve. The implementation should
replace the current format directly, update both committed recipes in the same
change, and delete the old parsing and evaluation path. Do not add a versioned
document enum, migration command, upgrade prompt or legacy recipe fixtures.

The current `schema_version` field and `CURRENT_SCHEMA_VERSION` check can be
removed until there is a real deployed compatibility requirement. Rust should
accept one unambiguous recipe shape and reject anything else through normal
deserialisation and validation.

### Current layer format

The root `material` remains the specialised base-height generator. The root
`albedo` block remains the base colour pass. This preserves the behaviour and
editability of the existing cracked-stone and rounded-stone recipes.

The current format replaces `surface_layers` with `layers`. Each layer has editor
identity, a scalar source, scalar remapping and independent output bindings. A
representative shape is:

```json
{
  "material": {
    "kind": "cracked_stone"
  },
  "layers": [
    {
      "id": "broad-colour-and-relief",
      "name": "Broad variation",
      "enabled": true,
      "source": {
        "kind": "fbm",
        "frequency": 3,
        "octaves": 4,
        "lacunarity": 2.0,
        "gain": 0.5,
        "offset": [0.0, 0.0],
        "seed_domain": 101,
        "domain_warp": null
      },
      "remap": {
        "input_min": -1.0,
        "input_max": 1.0,
        "invert": false,
        "contrast": 1.0,
        "bias": 0.0,
        "clamp": true
      },
      "mask": null,
      "outputs": {
        "height": {
          "enabled": true,
          "blend": { "kind": "add" },
          "strength_m": 0.012
        },
        "albedo": {
          "enabled": true,
          "blend": { "kind": "mix" },
          "strength": 0.3,
          "colour_map": {
            "kind": "gradient",
            "stops": [
              { "position": 0.0, "colour": [0.22, 0.24, 0.20] },
              { "position": 1.0, "colour": [0.42, 0.36, 0.28] }
            ]
          }
        }
      }
    }
  ]
}
```

The exact field names should be finalised alongside Rust types and golden
tests, but the responsibilities should remain separated as shown.

### Scalar source

The source produces a normalised scalar field. It contains:

- Kind: value, fBM, billow, ridged, cellular distance, cellular edge or
  cellular value.
- Whole-number tile frequency, presented as cells per tile.
- Octaves, lacunarity and gain for fractal sources.
- X/Y offset and independent seed domain.
- Cellular jitter for cellular sources.
- Optional domain warp with its own frequency, amplitude, octaves,
  lacunarity, gain and seed domain.

Domain warp should be one explicit source modifier because it is clearer to
edit and avoids deeply nested inspectors. The old recursive representation is
removed when the two recipes and Rust types are updated.

### Scalar remapping

Before routing, a layer can:

- Select an input range.
- Invert the result.
- Apply bias and contrast.
- Clamp or leave the value unbounded.
- Optionally apply a small monotonic curve represented by ordered control
  points.

The preview should show both the raw source and the remapped value.

### Height output

The height binding contains:

- Enabled state.
- Physical strength in metres.
- Replace, add, subtract, multiply, minimum, maximum or lerp blend.
- Lerp amount where relevant.

Using metres in the binding separates noise shape from physical displacement
and makes the same source useful for albedo without inheriting a height-only
amplitude.

### Albedo output

The albedo binding contains:

- Enabled state.
- Strength from zero to one.
- A colour mapping: two-colour ramp or multi-stop gradient in linear RGB.
- Blend mode: replace, mix, multiply, add or overlay.
- Optional hue, saturation and value influence for subtle variation without
  replacing the accumulated colour.

Albedo layers are applied after the existing base-albedo pass and before the
optional AO influence. Lighting must never be baked into albedo.

### Layer masks

A layer may be unmasked, use its own remapped scalar as opacity, use an inline
noise mask, or reference an earlier layer by stable ID. Mask invert, range and
contrast controls reuse the scalar-remapping model.

Deleting a referenced layer must present the references that will be broken.
Reordering a layer before one of its dependencies is rejected or moves the
dependency with it after confirmation.

### One-time update of the existing recipes

Update `cracked-stone.json` and `rounded-river-stones.json` directly:

| Current value | Replacement value |
| --- | --- |
| Array position | Same layer order |
| Noise `kind` and sampling fields | `source` |
| `amplitude` | `outputs.height.strength_m` |
| `blend` | `outputs.height.blend` |
| `domain_warp` | `source.domain_warp` |
| No current equivalent | Albedo output disabled |
| No current equivalent | Stable ID derived from layer purpose |

The base `material`, base `albedo`, displacement, normal, occlusion and output
settings remain conceptually unchanged. Capture the current map hashes before
the refactor, update the recipes, and confirm the new evaluator initially
reproduces them. This is a one-time development check, not a permanent old
format reader or migration facility.

## Rust work

### 1. Capture the visual baseline

- Generate small deterministic 128x128 outputs from the committed cracked stone
  and rounded river stone recipes and record hashes for
  height, albedo, normal, AO and packed mask.
- Use those hashes while replacing the format and evaluator.
- Once the new recipes match, make them the only fixtures and remove the old
  parser and old-format test inputs.
- Treat later unplanned hash changes as generator regressions.

### 2. Replace the recipe types and committed recipes

- Replace `surface_layers` and the old `NoiseLayer` shape with the new current
  layer model.
- Update validation and serde types directly rather than adding parallel old
  and new representations.
- Update both committed recipe JSON files in the same change.
- Remove `schema_version` if it no longer carries a useful contract.
- Delete the old evaluation path after the new recipe hashes match.
- Keep flat Rust module roots such as `procedural_textures.rs`; do not introduce
  `mod.rs` files.

Suggested files:

```text
island-rs/src/procedural_textures.rs
island-rs/src/procedural_textures/recipe.rs
island-rs/src/procedural_textures/layer_stack.rs
island-rs/src/procedural_textures/editor_protocol.rs
island-rs/src/procedural_textures/preview.rs
```

### 3. Implement layer evaluation

- Evaluate each scalar source once per pixel and retain it only while needed by
  height, albedo and masks.
- Apply scalar remapping once, then route the same result to enabled outputs.
- Fold height contributions in layer order.
- Start albedo with the existing base-albedo pass, then fold albedo
  contributions in the same order.
- Resolve earlier-layer masks by stable ID without cloning complete image
  buffers unnecessarily.
- Derive normal and AO from the final unquantised height field as today.
- Keep all sampling periodic and deterministic.

Use a small layer-result cache indexed by stable layer ID only when a later
mask needs it. Values with no future consumers should be released after their
height and albedo contributions have been applied.

### 4. Publish editor metadata from Rust

Add a machine-readable schema/metadata command generated from the Rust recipe
types. It needs:

- Property type, label, tooltip, default, numeric range and units.
- Tagged-enum alternatives and their conditional properties.
- Which noise controls apply to which source kinds.
- Which blend controls apply to which blend modes.

Prefer JSON Schema generated from Rust types plus a small Rust-owned UI metadata
table. Add a coverage test that fails when a new editable recipe property has
no label/range metadata. Unity requests this metadata when the studio opens;
there is no persistent schema cache or separate compatibility layer.

### 5. Extend the command-line protocol

Keep the current bake invocation and add explicit editor commands:

```text
island-texture-baker schema --json
island-texture-baker validate --recipe <file> --json
island-texture-baker preview --recipe <file> --output <temp-dir> --size 256 \
  --normal-convention <open-gl|direct-x>
```

All editor-facing commands return a JSON envelope containing success,
diagnostics, recipe hash, generated maps and timings. Human progress goes to
standard error so standard output remains parseable.

Preview adds:

- Resolution override without altering the saved recipe.
- Final albedo, height display, normal, AO and mask thumbnails.
- Raw scalar and remapped scalar thumbnails for a selected layer.
- A raw R16 height payload and metadata for the lit Unity preview.
- Atomic output with a completion manifest, just like final baking.

Validation diagnostics contain a JSON Pointer, severity, stable error code and
message so Unity can focus the corresponding control.

## Unity editor work

### 1. Replace the launcher with a document-oriented window

Build the new window with UI Toolkit using UXML and USS. Keep the existing menu
entry, redirecting it to the new studio.

Suggested layout:

```text
+--------------------------------------------------------------------------+
| New Open Save Save As Revert | Auto Preview | Validate | Bake            |
+----------------------+---------------------------+-----------------------+
| Base material        | Selected layer inspector  | Preview               |
|                      |                           | Albedo Height Normal   |
| Layer stack          | Source                    | AO Lit Layer           |
|  Base material       | Remap                     |                       |
|  Broad variation H A | Height output             | [single/tiled/3D]      |
|  Fine grain       H  | Albedo output             |                       |
|  Moss colour      A  | Mask                      |                       |
|                      |                           |                       |
| + Add  Duplicate     |                           |                       |
+----------------------+---------------------------+-----------------------+
| Validation, process status and bake output                               |
+--------------------------------------------------------------------------+
```

The layer list provides:

- Drag reorder.
- Enabled toggle.
- Editable name.
- `H` and `A` badges for height and albedo routing.
- Warning badge for invalid or unresolved masks.
- Add, duplicate and delete.
- Solo preview and before/after comparison.

The inspector shows only properties relevant to the selected source and blend
modes. Every numeric field has a slider where a useful range exists plus a
precise numeric entry. Controls have units, tooltips, reset-to-default and
context-menu copy/paste.

### 2. Add a loss-resistant document model

Create an in-memory `ScriptableObject` document containing:

- Source file path.
- Original file hash.
- Current JSON text/document.
- Rust schema metadata used to build the current controls.
- Dirty state and selected layer ID.
- Monotonic edit generation number.

Before each edit, call `Undo.RecordObject`; store the changed canonical JSON in
the object so Unity undo/redo can restore complete, valid document states. On
undo/redo, rebuild only the affected controls and request a preview.

Use atomic save through a temporary sibling followed by rename. If the source
file changed externally since opening, offer Reload, Save As or Overwrite rather
than silently replacing it. Revert and closing a dirty document require a
discard confirmation.

Suggested files:

```text
island-unity/Assets/Editor/ProceduralMaterials/
  ProceduralMaterialEditorWindow.cs
  ProceduralMaterialDocument.cs
  ProceduralMaterialSchema.cs
  ProceduralMaterialFormBuilder.cs
  ProceduralMaterialLayerList.cs
  ProceduralMaterialPreviewController.cs
  RustTextureBakerClient.cs
  ProceduralMaterialEditorWindow.uxml
  ProceduralMaterialEditorWindow.uss
```

### 3. Make Rust process execution asynchronous

Move process handling out of the window. `RustTextureBakerClient` should:

- Discover a configured release binary, then a known local release path.
- Allow Cargo fallback for explicit development bakes.
- Warn that Cargo fallback is unsuitable for live auto-preview.
- Redirect output without blocking Unity's main thread.
- Support cancellation and kill a superseded preview process.
- Marshal completion to the Unity main thread.
- Validate the expected response shape before using a result.
- Dispose processes and temporary files when the window closes or assemblies
  reload.

Only one preview process should run per document. Assign each request the
document edit-generation number and discard results from older generations.

### 4. Implement preview modes

The preview pane has these tabs:

- **Albedo**: lighting-free final colour.
- **Height**: fixed recipe range or auto-levelled greyscale, with min/max values.
- **Normal**: normal-map display using the selected convention.
- **Occlusion**: final AO.
- **Mask**: packed output with channel selector.
- **Layer**: raw, remapped and masked contribution for the selected layer.
- **Lit**: final maps on an interactive preview object.

The 2D viewer supports one tile, 2x2 tiling to expose seams, zoom, pan and pixel
inspection. It displays the physical tile dimensions and preview resolution.

The lit viewer uses `PreviewRenderUtility` and a dedicated hidden preview
shader. It provides plane and sphere meshes, orbit, zoom, light direction and
strength, and toggles for albedo, normal, height and AO. A subdivided plane plus
the raw R16 preview height can show real displacement without changing the
production shader.

Auto-preview behaviour:

- Default resolution 256x256, selectable 128, 256 or 512.
- Debounce changes for roughly 300 ms.
- Cancel a running request when a newer edit arrives.
- Cache by normalised recipe hash, selected layer and preview settings.
- Provide a manual Preview button and an Auto Preview toggle.
- Never auto-run a full-resolution bake.

### 5. Keep baking transactional

The Bake panel remains separate from preview and retains the current safeguards:

- Output must be below `Assets/Generated/Textures`.
- Existing output requires explicit Replace Existing Set.
- Import only after a successful manifest is present.
- Configure sRGB, linear, normal-map, repeat, mipmap and compression settings by
  map purpose.
- Assign to a selected material only after every required map imports.
- Record material assignment with Unity Undo.

Add a recipe snapshot or recipe hash to each generated set so the editor can
show whether the current document differs from the last bake.

## Validation strategy

### Rust automated tests

- Both updated recipes load through the single current recipe type.
- The updated recipes preserve the five baseline map hashes.
- Every noise source is periodic and deterministic through the current layer
  path.
- Every height and albedo blend mode has focused numerical tests.
- A layer can affect height only, albedo only, both or neither.
- Earlier-layer masks resolve correctly; missing, forward and cyclic references
  fail validation.
- Preview at a given resolution matches a final bake at that resolution.
- Schema metadata covers every editable field.
- JSON diagnostics contain stable paths and codes.
- CLI protocol output remains valid JSON even when generation fails.

Run formatting, focused tests, `cargo test --all-targets`, locked checks and
strict Clippy. Keep the known unrelated river-continuity failure reported
separately if it still exists.

### Unity EditMode and batch tests

- Open both updated committed recipes and populate all controls.
- Save an untouched document without semantic changes.
- Change each tagged source/material variant and populate the correct controls.
- Add, duplicate, reorder, disable and delete layers.
- Undo and redo field edits and layer operations.
- Surface Rust validation errors on the correct control.
- Ignore stale preview results after a newer edit.
- Cancel preview safely on close and assembly reload.
- Verify preview files remain outside `Assets`.
- Bake, import and assign a generated set transactionally.

Add a batch entry point alongside the current Island validation utilities so
CI can load the schema, round-trip both recipes, run a small preview, inspect
the manifest and verify the window's service layer without manual interaction.

### Manual Unity acceptance

Test with Unity 6000.5.6f1:

1. Open each existing recipe and compare a fresh bake with the committed maps.
2. Edit every base-material section and confirm the preview changes.
3. Build a three-layer material with one height-only, one albedo-only and one
   combined layer.
4. Exercise every noise and blend kind.
5. Reorder and mask layers while preview generation is running.
6. Inspect single-tile and 2x2 seam views.
7. Inspect the displaced plane and lit sphere.
8. Save, close, reopen and confirm all settings persist.
9. Bake and assign to rock and riverbed slots, then run the existing terrain and
   material batch validation.

## Performance and usability targets

- Editing and scrolling should not block the Unity main thread.
- With a release baker, a 256x256 preview should normally appear within one
  second; cached previews should appear immediately.
- Preview memory should remain bounded by a small LRU cache, initially eight
  complete previews.
- No preview asset should trigger `AssetDatabase.Refresh`.
- A full bake is always explicit and reports elapsed time per stage.
- Validation should complete before launching a preview and should focus the
  first invalid control.
- The last ten preview timings and the current cache state should be available
  in a collapsible diagnostics section.

## Implementation sequence

### Phase 1: baseline and editor protocol

Deliver baseline hashes, machine diagnostics, schema output and
asynchronous-process protocol tests. Do not change recipe output.

Exit criterion: both current recipes have locked hashes and Unity can request
schema and validation JSON from a release baker.

### Phase 2: replace the layer model and recipes

Deliver the new recipe shape, scalar remapping, independent height/albedo
bindings, masks and the updated committed recipes. Remove the old shape and
evaluator in this phase.

Exit criterion: the updated existing recipes retain their baseline hashes and
focused tests cover all source and blend variants.

### Phase 3: Unity document editor

Deliver the JSON document model, schema-driven controls, layer stack, file
operations, dirty-state handling and undo/redo. Retain the existing Bake path.

Exit criterion: both existing recipes can be opened, completely edited and
saved without using a text editor.

### Phase 4: asynchronous 2D preview

Deliver debounced/cancellable preview generation, cache, map tabs, selected
layer diagnostics and tiled seam view.

Exit criterion: rapid edits never freeze Unity or display an older result over
a newer one.

### Phase 5: lit preview and final bake integration

Deliver the interactive plane/sphere preview, displacement plane, last-bake
comparison, import and material-assignment workflow.

Exit criterion: one recipe can move from creation through preview, save, bake,
import and terrain assignment within the studio.

### Phase 6: hardening and documentation

Deliver EditMode/batch coverage, keyboard navigation, tooltips, error focusing,
performance telemetry, current-format documentation and an artist workflow
guide.

Exit criterion: Rust checks, Unity batch validation and the manual acceptance
script pass on both existing recipes.

## Risks and controls

| Risk | Control |
| --- | --- |
| Rust and Unity schemas drift | Rust-generated schema metadata; Unity patches a JSON DOM rather than owning DTOs |
| Existing recipes change appearance | Capture hashes before the refactor and require the directly updated recipes to match |
| Preview differs from final bake | Same Rust evaluator and settings; only resolution is overridden |
| Unity freezes during generation | Asynchronous process service, cancellation and generation IDs |
| Rapid edits show stale output | Discard responses whose generation ID is not current |
| A save overwrites external edits | Detect source-file hash changes and require Reload, Save As or explicit Overwrite |
| Mask references create a graph cycle | Earlier-layer references only, validated by stable ID |
| Preview assets pollute source control | Store temporary data under `Library`, never `Assets` |
| Auto-preview repeatedly invokes Cargo | Require/discover a release binary for auto mode; Cargo fallback is manual |
| The editor becomes too complex | Ordered stack, progressive disclosure and relevant-controls-only inspectors |

## Completion criteria

The work is complete when:

- The committed cracked-stone and rounded-stone recipes open and reproduce
  their current outputs.
- An artist can create, edit, reorder, duplicate, disable and remove noise
  layers without editing JSON.
- Every layer can independently affect height, albedo, both or neither.
- Interactions and masks are visible and understandable in the layer stack.
- 2D map, tiled seam, selected-layer and lit 3D previews are available.
- Previewing is asynchronous, cancellable and does not import assets.
- Save is validated and atomic, and detects external file changes.
- Final baking remains deterministic, transactional and engine-neutral.
- Rust, Unity batch and manual acceptance checks all pass, with unrelated existing
  failures reported separately rather than hidden.

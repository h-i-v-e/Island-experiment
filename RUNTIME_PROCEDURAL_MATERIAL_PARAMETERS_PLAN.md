# Runtime Procedural Material Parameters Plan

Status: implemented through the shared baker, studio/CLI, Unity adapter, and Bevy runtime consumer; hardening and visual tuning remain iterative

Primary outcome: the engine chooses dirt and stone colours for an island and passes them to the Rust library as explicit bake inputs. The library applies those values to parameterised procedural recipes, bakes the final texture maps in memory, and returns owned texture buffers. Unity, Bevy, the native material studio, and the CLI all use that same library path.

Runtime rebaking is explicitly acceptable. The design therefore does not require palette-weight textures or shader-side reconstruction of recipe colours. The engine receives final coloured albedo maps plus the associated normal, height, occlusion, and packed-mask data.

## Goals

- Let each engine choose or randomise its own coherent dirt and stone colours.
- Require the engine to pass those colours explicitly when requesting textures.
- Use exactly the same palette values in procedural recipes and terrain shader fallbacks.
- Let recipes declare typed inputs and bind colour fields to those inputs.
- Bake recipes through an engine-neutral Rust library API and return textures in memory.
- Keep PNG and manifest writing optional and outside the core bake path.
- Preserve all existing literal-only recipe output exactly.
- Make the same capability available to Unity, Bevy, the native material studio, and the texture baker CLI.
- Preserve deterministic output across machines, terrain generation methods, threading, and engine adapters.
- Make texture ownership, cancellation, caching, and destruction explicit.

## Non-goals for the first implementation

- A general expression language or arbitrary recipe scripting system.
- Shader-side procedural recipe evaluation.
- Per-frame or continuously animated rebaking.
- Parameterising every numeric field in the recipe format at once.
- Network distribution of baked maps.
- Removing the existing standalone file baker.
- Making the FFI ABI the owner of recipe parsing or baking logic.
- Generating, randomising, persisting, or deriving an island palette inside `island-rs`.

## Architectural decision

The Rust library is the sole owner of parameter resolution and texture generation:

```text
engine island state / seed
    |
    v
engine-selected dirt and stone colours
    |
    +--> shader constants and non-textured fallbacks
    |
    v
RuntimeMaterialInputs + embedded TextureRecipe
    |
    v
resolve_texture_recipe()
    |
    v
ResolvedTextureRecipe
    |
    v
generate_texture_set() / bake_island_materials()
    |
    v
owned TextureSet values
    |
    +--> native material studio preview/save
    +--> CLI image writer
    +--> Bevy Image upload
    +--> C ABI adapter --> Unity Texture2D upload
```

The core API is synchronous. It performs deterministic CPU work and returns owned data. Callers decide how to schedule it:

- the native studio uses Bevy's compute task pool;
- the Bevy island viewer uses a bounded background task;
- Unity calls it from the existing island-generation worker and uploads on the main thread;
- tests and command-line tools can call it directly.

This keeps palette policy, async runtimes, engine handles, persistence, and thread-affine graphics APIs out of `island-rs`.

## Current state to preserve

- `TextureRecipe` is strict and uses `deny_unknown_fields`.
- Recipe albedo colours are currently literal linear RGB arrays.
- `evaluate_material` and `generate_texture_set` are shared Rust entry points.
- `TextureSet` already owns the generated albedo, normal, displacement, occlusion, and packed map images.
- `write_texture_set` is already separable from generation.
- The native material studio and CLI use the shared evaluator.
- Unity currently consumes committed generated textures and adds several shader colour multipliers.
- Unity currently derives some material colours from average texture colours; the explicit engine-selected inputs must replace that as the palette authority.
- Bevy currently has its own terrain rendering path and will require explicit bindings before it can display the same material texture sets.

Before changing the schema or evaluator, capture small deterministic output hashes for every committed recipe. Literal-only recipes and parameterised recipes using their old default values must reproduce those hashes exactly.

## 1. Typed recipe parameters

### 1.1 Initial parameter types

Start with one deliberately narrow type:

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ParameterDefinition {
    Colour {
        default: LinearRgb,
        #[serde(default)]
        description: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParameterValue {
    Colour(LinearRgb),
}
```

Use a small `LinearRgb` value type rather than passing unlabelled `[f32; 3]` through public APIs. It must validate finite values and the inclusive `0.0..=1.0` range.

Future scalar, integer, boolean, enum, or seed-offset parameters can be added as new tagged variants without introducing an expression evaluator now.

### 1.2 Recipe declarations

Add an optional, ordered parameter map to `TextureRecipe`:

```rust
#[serde(default)]
pub parameters: BTreeMap<String, ParameterDefinition>,
```

`BTreeMap` gives stable serialisation, diagnostics, and hashes. Names use lower snake case and have conservative length/count limits. The initial shared semantic names are:

- `dirt_colour`
- `stone_colour`

Names remain recipe-local. Their shared spelling is the convention that lets an engine supply the same values to several recipes.

Example:

```json
{
  "parameters": {
    "dirt_colour": {
      "kind": "colour",
      "default": [0.118, 0.064, 0.029],
      "description": "Linear RGB colour of exposed soil"
    },
    "stone_colour": {
      "kind": "colour",
      "default": [0.31, 0.29, 0.25],
      "description": "Linear RGB colour of the local stone"
    }
  }
}
```

### 1.3 Colour bindings

Replace recipe fields that currently accept only `[f32; 3]` with a backward-compatible `ColourValue`:

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ColourValue {
    Literal(LinearRgb),
    Parameter(ColourParameterReference),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColourParameterReference {
    pub parameter: String,
    /// Optional authored anchor. At the declared default it resolves exactly
    /// to this value; an override tints it relative to that default.
    pub base: Option<LinearRgb>,
}
```

Existing arrays remain valid literals. A binding is explicit:

```json
"colour": { "parameter": "stone_colour", "base": [0.31, 0.29, 0.25] }
```

The optional `base` is what preserves useful differences between authored
gradient stops while letting one engine colour move the whole family. With no
`base`, the resolved colour is the parameter value directly. The first
supported binding sites are:

- albedo base and warm colours;
- palette entries;
- colour-ramp endpoints;
- gradient-stop colours.

Do not parameterise masks, height values, frequencies, or layer geometry in the first pass. Those can use the same type system later if there is a concrete use case.

### 1.4 Overrides and defaults

`RecipeParameterValues` is an owned or borrowed map passed to resolution. Resolution follows these rules:

1. An explicit override wins.
2. A declared default is used when no override is supplied.
3. A reference to an undeclared name is an error.
4. An override for an undeclared name is an error by default.
5. Type mismatches, non-finite values, and out-of-range colours are errors.
6. All errors include the recipe JSON pointer and parameter name.

Strict unknown-override handling prevents a misspelt `stone_colour` from silently generating a plausible but incorrect island.

## 2. Resolution and evaluator boundary

Introduce an owned `ResolvedTextureRecipe`. It contains only concrete, validated values and is the only recipe form accepted by the internal evaluator.

```rust
pub fn resolve_texture_recipe(
    recipe: &TextureRecipe,
    values: &RecipeParameterValues,
) -> Result<ResolvedTextureRecipe, RecipeParameterErrors>;
```

The resolver must be pure and deterministic. It must not access the filesystem, global random state, engine state, or environment variables.

Keep the existing public convenience API compatible:

```rust
pub fn generate_texture_set(
    recipe: &TextureRecipe,
    normal_convention: NormalConvention,
) -> Result<TextureSet, TextureError>;
```

It resolves declared defaults and then takes the same internal bake path. Add the parameter-aware sibling:

```rust
pub fn generate_texture_set_with_parameters(
    recipe: &TextureRecipe,
    parameters: &RecipeParameterValues,
    normal_convention: NormalConvention,
) -> Result<TextureSet, TextureError>;
```

Also expose advanced stages for the studio and tests:

```rust
pub fn evaluate_resolved_material(
    recipe: &ResolvedTextureRecipe,
) -> Result<MaterialEvaluation, TextureError>;

pub fn texture_set_from_evaluation(
    recipe: &ResolvedTextureRecipe,
    evaluation: &MaterialEvaluation,
    normal_convention: NormalConvention,
) -> Result<TextureSet, TextureError>;
```

The evaluator must not repeatedly perform map lookups per texel. All references are replaced by concrete colours once during resolution.

Suggested new files, without introducing `mod.rs`:

- `island-rs/src/procedural_textures/parameters.rs`
- `island-rs/src/procedural_textures/runtime_materials.rs`

Declare both from the existing `procedural_textures.rs` module.

## 3. Library-owned material baking

### 3.1 Explicit engine inputs

Add a narrow engine-neutral request type for the shared runtime recipe inputs:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeMaterialInputs {
    pub dirt_colour: LinearRgb,
    pub stone_colour: LinearRgb,
}
```

The library validates these values and converts them into recipe-local `RecipeParameterValues`. It does not generate them, mutate them, attach them to `Island`, infer them from the island seed, or persist them. The caller retains the authoritative copy.

Keep the input borrowed at the synchronous boundary. The returned textures are owned because they must outlive the request and cross engine task/upload boundaries.

### 3.2 Embedded recipe registry

Runtime baking must not depend on a source checkout or loose JSON files beside an executable. Embed the approved runtime recipe templates in the Rust library with `include_str!`, parse and validate them once, and expose them through a typed registry:

```rust
pub enum IslandMaterialKind {
    Rock,
    RiverBed,
    ForestFloor,
    FallenStones,
}
```

The registry should initially include only materials actually consumed by runtime terrain shaders. Adding a recipe requires an explicit enum case, binding contract, and test rather than a magic filename.

### 3.3 High-level return API

The high-level library call bakes a caller-consistent group and returns all data in memory:

```rust
pub struct RuntimeMaterialBakeOptions {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub normal_convention: NormalConvention,
    pub materials: MaterialSelection,
}

pub struct IslandMaterialTextures {
    pub materials: BTreeMap<IslandMaterialKind, TextureSet>,
    pub identity: MaterialBakeIdentity,
}

pub fn bake_island_materials(
    inputs: &RuntimeMaterialInputs,
    options: &RuntimeMaterialBakeOptions,
) -> Result<IslandMaterialTextures, RuntimeMaterialBakeError>;
```

The function supports engines, tools, and tests. It borrows the input colours and options, returns owned textures, and writes nothing to disk. There is deliberately no `Island::bake_material_textures` convenience method because palette selection is not part of the library's island model.

`width` and `height` are controlled resolution overrides applied to a temporary resolved bake description. They must not mutate the embedded recipe or its identity accidentally.

### 3.4 Core versus adapter outputs

The core `TextureSet` remains semantically rich and engine-neutral:

- sRGB-encoded albedo bytes with explicit colour-space metadata;
- tangent-space normal bytes plus normal convention;
- height/displacement samples;
- occlusion samples;
- any existing generated packed maps.

Engine adapters may derive upload-oriented buffers, for example Unity's dual-material RGBA mask packing, but those layouts do not replace or leak into the core recipe evaluator.

PNG encoding and manifest writing remain explicit calls to `write_texture_set`. A runtime bake must never encode and immediately decode PNGs.

## 4. Engine-owned palette selection

Colour generation is an engine responsibility and is outside `island-rs`.

Each engine should:

- choose or randomise dirt and stone colours before starting the material bake;
- retain those authoritative colours in its island/runtime state;
- pass the same linear RGB values to `bake_island_materials` and its terrain shader fallbacks;
- own any seed derivation, randomisation ranges, artist controls, save migration, and palette versioning it requires;
- include the explicit colour values or their canonical hash in engine-side cache keys;
- convert UI or configuration colours from sRGB to linear RGB before calling the library.

Unity and Bevy may use different palette-generation policies. The library guarantees that identical explicit inputs and bake options produce identical texture bytes; it does not guarantee that two engines independently derive the same colours from the same island seed.

If cross-engine visual parity is later required, share a palette preset/data file or move a small optional generator into a separate crate. Do not put generation policy into the core procedural texture library.

## 5. Recipe migration

Migrate in small groups and preserve each recipe's current colours as declared defaults.

| Recipe/material | `dirt_colour` use | `stone_colour` use |
| --- | --- | --- |
| Fallen stones | exposed soil and dirt variation anchors | stone cluster ramp anchors |
| Forest floor | soil visible between litter and low vegetation | optional small-stone details only |
| River bed | sediment/interstitial bed colour | pebbles and larger stones |
| Rock terrain | weathered dirt staining where present | primary rock palette |

Do not replace useful within-material variation with one flat colour. Parameter values should be colour anchors; existing height, noise, HSV influence, ramps, and weathering still create local variation around them.

For each migration:

1. Declare the parameters with defaults matching the current literal values.
2. Replace only the intended colour anchors with references.
3. Bake with defaults and compare exact baseline maps.
4. Override dirt only and prove normal, height, and occlusion maps are unchanged.
5. Override stone only and inspect the intended material regions.
6. Add the recipe to the runtime registry only after visual approval.

Bark should remain separate until there is an explicit decision about which additional engine-supplied colour inputs it requires.

## 6. Bake identity, manifests, and caches

Separate the following identities:

- `template_hash`: canonical recipe JSON including parameter declarations and references;
- `parameter_hash`: canonical typed resolved values in sorted-name order;
- `material_hash`: template hash plus parameter hash plus effective resolution and algorithm version;
- `normal_convention`: recorded separately, as it changes normal bytes but not the underlying material evaluation;
- `algorithm_version`: evaluator/encoder behaviour version.

Extend `TextureMetadata` and `OutputManifest` with:

- declared parameter definitions or their template hash;
- canonical resolved parameter values;
- parameter hash;
- effective material hash;
- no palette-generation metadata, because generation policy belongs to the caller.

Do not include timestamps or output paths in a deterministic hash.

Runtime caches use a key containing material kind, template hash, parameter hash, resolution, algorithm version, and normal convention. A cache hit must return data equivalent to a fresh bake.

An in-memory cache is sufficient for the first implementation. A later on-disk cache can store encoded engine-ready data, but cache corruption or version mismatch must fall back to rebaking.

## 7. Caller persistence and compatibility

The library does not store colour choices in `Island` or alter the island save format. Each engine decides whether its generated colours are ephemeral or persisted.

Recommended engine policy when island appearance must survive reloads:

- persist the selected linear dirt and stone values in engine-owned island state;
- persist the engine's palette-policy version if it may regenerate missing values;
- pass the stored values back to the library on every rebake;
- include the values or `parameter_hash` in engine-side development caches;
- define an explicit migration for older engine saves that do not contain colours;
- ensure changes to engine randomisation do not silently recolour saved islands unless that is intended.

The recipe format can remain backward compatible because missing `parameters` means an empty map and literal arrays remain valid. Add an explicit recipe format version only if a later change cannot be represented compatibly.

## 8. Native material studio

Add a Parameters panel containing:

- parameter name;
- type;
- default value;
- description;
- current preview override;
- validation state;
- add, rename, and remove actions.

Every supported colour editor gets a binding selector:

- `Literal` shows the existing colour picker;
- `Parameter` shows a compatible parameter dropdown and resolved swatch.

Behavioural rules:

- declaration or binding edits change the document and participate in undo/redo;
- preview overrides are session state and do not dirty the recipe;
- `Promote override to default` is an explicit document edit;
- deleting or renaming a referenced parameter requires an atomic reference update or is rejected with all affected paths listed;
- preview and Bake use `generate_texture_set_with_parameters`;
- the preview cache key includes the effective parameter hash;
- the Bake action continues to write files only because the user explicitly selected an output destination.

The studio may offer named or manually entered parameter presets to test a range of colour combinations. It should not contain an authoritative island palette randomiser; the actual engine remains responsible for choosing runtime colours.

## 9. CLI and schema protocol

Update `editor_protocol.rs`, validation, and the CLI together so the studio does not invent a second schema.

Add CLI inputs such as:

```text
texture_baker preview --recipe FallenStones.json \
  --parameters island-palette.json --output preview

texture_baker preview --recipe FallenStones.json \
  --set-colour dirt_colour=#6d4a32 \
  --set-colour stone_colour=#77736d \
  --output preview
```

Hex input is documented as sRGB and converted to linear RGB. JSON parameter files should use the typed canonical representation and clearly identify their colour space.

The schema/metadata protocol must describe:

- parameter declarations;
- parameter reference objects;
- compatible binding sites;
- defaults and descriptions;
- diagnostics for missing, unknown, malformed, or incorrectly typed overrides.

CLI JSON output includes resolved values and bake identity so automated callers can verify what was produced.

## 10. Unity integration

Unity remains an adapter and renderer; it does not parse recipes.

### 10.1 C ABI

Add a stable C representation over the library call. Prefer one bake handle owning all returned buffers rather than many independently allocated pointers:

```text
MotuBakeIslandMaterials(inputs, options, out bake_handle, out descriptor)
MotuGetBakedMaterial(bake_handle, material_kind, out texture_descriptor)
MotuReleaseMaterialBake(bake_handle)
```

`inputs` contains explicit linear dirt and stone colours supplied by Unity. The descriptor records dimensions, formats, byte lengths, normal convention, resolved parameter hash, and bake identity. Every returned pointer remains valid until the single bake handle is released.

Requirements:

- explicit fixed-width integer fields and `repr(C)` layouts;
- no Rust enum layout crosses the ABI without a fixed representation;
- null checks and error codes for invalid handles or unsupported formats;
- one safe release operation, with C# ownership wrapped in `IDisposable`/safe-handle style code;
- no PNG data and no callback into Unity from Rust;
- no Unity object creation off the main thread.

The FFI implementation converts the fixed-layout colour inputs to `RuntimeMaterialInputs` and calls `bake_island_materials`; it must not duplicate recipe selection or parameter mapping. It contains no randomisation logic and does not require an island handle.

### 10.2 Generation worker

Have Unity choose its palette before material baking, then run the library call in the existing generation worker with those explicit colours. Carry both the authoritative engine colours and returned buffers in `IslandPreparedData`, then create and upload `Texture2D` objects on Unity's main thread.

Use bounded concurrency. A 1024-by-1024 recipe evaluation holds substantially more temporary float data than its final texture bytes; baking every material simultaneously could cause a large memory spike. Start with one or two concurrent material bakes and measure before increasing it.

Support cancellation between material bakes and before upload. Rebuilding an island must dispose old native bake handles, managed buffers, Unity textures, and materials exactly once.

### 10.3 Shader/material contract

Set terrain shader properties from the same Unity-owned colour values passed to the bake request:

- `_GroundDirtColor` uses `dirt_colour`;
- `_RockColor` and untextured rock decoration fallbacks use `stone_colour`;
- riverbed, forest-floor, and fallen-stone texture samplers receive the newly baked maps;
- multipliers on already final-coloured albedo maps default to white unless they represent an intentional independent effect.

Remove `ApplyAverageTextureColor` as an authority for island colours. It may remain temporarily as a debug comparison, but runtime behaviour must not depend on averaging an output texture back into the value that generated it.

Retain runtime channel packing only as an upload optimisation. Its inputs come from the returned `TextureSet` data rather than imported Unity assets.

## 11. Bevy integration

The native studio already uses the Rust baker, but the island viewer's terrain material needs explicit runtime texture support.

Add a background material-bake task that:

1. receives owned Bevy-selected colour inputs and bake options;
2. calls `bake_island_materials` off the render/main schedule;
3. returns `IslandMaterialTextures` through a task result;
4. creates Bevy `Image` assets on the main ECS world;
5. binds them to the terrain material extension;
6. drops temporary CPU images after upload when readback is unnecessary.

Extend the WGSL and material bind group with the same semantic material inputs as Unity. Keep binding names based on material meaning rather than Unity property names.

Until the bake completes, render with the engine-selected colours as fallbacks rather than a black material. Swap the full texture set atomically to avoid a frame containing a mixture of old and new island maps.

Metal binding limits must be checked. Reuse the existing packed occlusion/height strategy where necessary, and test on Metal because the project's Bevy path has already encountered binding capability differences there.

## 12. Concurrency, memory, and lifecycle

- Core bake calls borrow immutable recipes and parameter maps and return owned results.
- Background task closures own only the engine-selected colour inputs, options, and recipe data needed for their lifetime.
- No graphics resource or Unity/Bevy world reference crosses into the core baker.
- Bound runtime bake concurrency to an empirically measured value.
- Report progress by material count rather than adding callbacks inside the evaluator initially.
- Check cancellation between recipes; deeper evaluator cancellation can be added only if individual recipe latency becomes unacceptable.
- Keep the last complete material set active until the next complete set is uploaded.
- Treat a partial multi-material bake as an error and do not publish it to the renderer.
- Cache immutable completed results, not mutable evaluation workspaces.

Add release-mode benchmarks at 512 and 1024 resolution for one recipe and the complete runtime set. Record peak resident memory as well as elapsed time.

## 13. Validation and tests

### Recipe and parameter tests

- legacy literal recipe parses unchanged;
- missing `parameters` behaves as an empty map;
- declared defaults resolve correctly;
- explicit overrides win;
- undeclared references fail at the correct JSON pointer;
- unknown overrides fail;
- type/range/non-finite validation fails clearly;
- serialisation and hashing are stable regardless of insertion order;
- removing/renaming referenced parameters is rejected or atomically repaired by the studio.

### Evaluator regression tests

- capture existing low-resolution hashes before implementation;
- legacy recipes remain byte-identical;
- a migrated recipe with default parameters remains byte-identical;
- changing a colour parameter changes only albedo-derived outputs;
- height, normal, and occlusion outputs remain identical for colour-only changes;
- studio preview, final generation, high-level runtime bake, and CLI produce the same bytes for the same inputs.

### Explicit-input tests

- the same explicit colours and options produce the same texture bytes;
- CPU/GPU terrain selection, thread count, and task order do not affect library output;
- omitted, malformed, non-finite, or out-of-range engine inputs fail clearly;
- input structs are not mutated by a bake;
- dirt and stone values supplied to every recipe equal the engine shader values bit-for-bit after the defined colour-space conversion;
- engine save/load and randomisation tests live in their respective Unity or Bevy integration suites, not in the texture library.

### FFI and engine tests

- ABI layout and fixed-width field assertions;
- invalid handle/kind/options return errors without leaking;
- all buffer lengths match dimensions and formats;
- one release frees all returned buffers;
- Unity creates textures only on the main thread and destroys them on regeneration;
- linear versus sRGB texture import/upload flags are correct;
- OpenGL/DirectX normal convention selection remains explicit;
- Bevy and Unity render the same material relationships when given the same explicit input colours;
- shader compilation succeeds on Metal and the supported Unity target;
- runtime rebake never relies on editor-only assets or APIs.

### Performance acceptance

- island generation does not block the render/main thread during recipe evaluation;
- bounded concurrency stays under the agreed memory budget;
- a cache hit avoids reevaluation;
- repeated island rebuilds do not grow native or GPU memory;
- failed or cancelled bakes leave the previous complete material set usable.

## 14. Implementation phases

### Phase 0: baselines and contracts

- Inventory all runtime material recipes and shader consumers.
- Capture deterministic map hashes for every committed recipe at a small test resolution.
- Record current release bake time and peak memory at 512 and 1024.
- Write the shared semantic mapping from engine colour inputs to recipes and shader properties.

Exit criterion: current output and runtime contracts are recorded before schema changes.

### Phase 1: parameter schema and resolver

- Add `LinearRgb`, definitions, values, references, and `ResolvedTextureRecipe`.
- Extend validation and JSON-pointer diagnostics.
- Keep literal arrays backward compatible.
- Route evaluator entry through resolution.
- Prove literal recipes remain byte-identical.

Exit criterion: defaults and overrides work through Rust tests without changing current recipes.

### Phase 2: public library bake API

- Add `generate_texture_set_with_parameters`.
- Add runtime recipe registry and `IslandMaterialKind`.
- Add `bake_island_materials` returning owned `TextureSet` values.
- Keep all file encoding optional.
- Add effective identities and parameter metadata.

Exit criterion: a Rust integration test bakes the complete material group in memory with no filesystem access.

### Phase 3: recipe migrations

- Parameterise Fallen Stones.
- Parameterise Forest Floor.
- Parameterise River Bed.
- Parameterise Rock.
- Preserve old colours as defaults and approve representative palette variations.

Exit criterion: default outputs pass baselines and alternate palettes visibly affect only intended albedo regions.

### Phase 4: engine colour ownership

- Define Unity's colour selection/randomisation policy and storage location.
- Define Bevy's colour selection/randomisation policy and storage location.
- Add engine-side persistence or regeneration rules where required.
- Add engine-side cache identity and representative colour-quality tests.
- Confirm neither path adds palette generation or palette state to `island-rs`.

Exit criterion: each engine can produce and retain explicit dirt/stone values, and the library accepts those values without knowing how they were chosen.

### Phase 5: native studio and CLI

- Add declarations, binding controls, and preview overrides to the studio.
- Update schema metadata and diagnostics.
- Add CLI parameter-file and colour override support.
- Update bake manifests and documentation.

Exit criterion: the studio and CLI produce the same bytes as direct library calls.

### Phase 6: Unity adapter

- Add the bake-handle C ABI over the library API.
- Carry returned buffers through `IslandPreparedData`.
- Upload textures on the main thread and dispose them safely.
- Replace static generated-map dependencies for runtime island materials.
- Wire Unity-owned colours to shader fallbacks and remove average-colour authority.

Exit criterion: a Unity runtime build creates two seeds with visibly different but internally matching dirt/stone palettes without editor assets.

### Phase 7: Bevy renderer

- Add bounded background baking.
- Upload returned maps to Bevy `Image` assets.
- Extend terrain material bindings/WGSL.
- Add engine-colour fallback and atomic material-set swap.
- Validate Metal texture limits and packing.

Exit criterion: Bevy and Unity consume texture sets produced by the same Rust API and render the same known palette relationships.

### Phase 8: hardening

- Add caching, cancellation, progress, and memory benchmarks.
- Stress repeated generation and teardown.
- Complete format/API documentation and examples.
- Remove temporary compatibility/debug paths only after both engines pass.

Exit criterion: deterministic, leak-free runtime rebaking meets the agreed latency and memory budget.

## 15. Recommended delivery slices

Keep commits reviewable and reversible:

1. Baseline fixtures and golden hashes.
2. Parameter types, schema, resolver, and validation.
3. Evaluator integration and parameter-aware single-recipe library API.
4. Runtime registry and multi-material return API.
5. Fallen Stones and Forest Floor migration.
6. River Bed and Rock migration.
7. Unity and Bevy colour selection, ownership, and persistence.
8. Studio and CLI authoring support.
9. FFI ownership layer.
10. Unity worker/upload/shader integration.
11. Bevy task/upload/shader integration.
12. Cache, cancellation, performance, and documentation.

Do not combine the recipe-schema change, engine persistence changes, FFI ABI, and both renderer integrations in one commit. Each boundary should be independently testable.

## Completion criteria

The feature is complete when:

- each engine selects and owns the dirt and stone colours it wants for an island;
- each engine passes those explicit colours when requesting textures;
- `island-rs` contains no palette randomisation, seed derivation, or palette persistence policy;
- the Rust library can bake one recipe or the complete runtime material set with explicit parameters;
- the library returns owned textures without filesystem I/O;
- file writing remains available as a separate tool operation;
- every relevant recipe uses the common semantic palette names;
- terrain shader fallback colours and recipe overrides originate from the same engine-owned input values;
- legacy/default recipes retain their accepted output;
- Unity and Bevy perform baking off their main threads and upload only complete texture sets;
- runtime rebuild, cancellation, failure, and teardown do not leak native or GPU resources;
- manifests and cache keys fully identify the template, parameters, resolution, algorithm, and normal convention;
- focused Rust tests, strict formatting/linting, Unity batch validation, Bevy checks, Metal shader validation, and visual seed comparisons all pass, with unrelated pre-existing failures reported separately.

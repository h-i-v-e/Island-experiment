# Procedural Material Studio

Procedural Material Studio is the Unity authoring front end for the
engine-neutral Rust texture baker. Unity edits the portable JSON document and
handles imported assets; Rust remains authoritative for the schema,
validation, procedural fields, colour, normals, occlusion, packing and output.

## Before opening the studio

Build the release baker from the repository root:

```sh
cargo build --release --locked \
  --manifest-path island-rs/Cargo.toml \
  --bin island-texture-baker
```

Open `island-unity` with Unity 6000.5.6f1, then choose
`Island > Terrain > Procedural Material Studio`. The studio discovers the
release executable automatically. Cargo fallback is available for explicit
manual Preview or Bake operations, but is intentionally disabled for live
auto-preview.

## Authoring workflow

1. Open `cracked-stone.json`, `rounded-river-stones.json`, or create a recipe.
2. Edit the base material and output settings in the middle inspector.
3. Add, duplicate, rename, enable, delete or drag layers in the left stack.
4. Select a source, remap its scalar, then route it independently to height,
   albedo, both or neither. A layer mask can use its own scalar, inline noise,
   or an earlier layer by stable ID.
5. Use Albedo, Height, Normal, Occlusion, Mask and Layer tabs for unlit maps.
   Enable the 2x2 view to inspect tile seams. The Lit tab supports a sphere or
   displaced plane, orbit/zoom, map toggles and adjustable lighting.
6. Leave Auto Preview enabled for debounced 128, 256 or 512 previews. Preview
   output and the eight-entry cache stay under `Library` and never enter the
   Asset database. The diagnostics foldout shows the last ten Rust timings.
7. Use Validate before saving when investigating an issue. Diagnostics can be
   clicked to select the affected layer and focus its control. Fields provide
   reset, copy-JSON and paste-JSON context-menu actions.
8. Save or Save As. Rust validation runs before the atomic file replacement;
   external file changes require Reload, Save As or explicit Overwrite.
9. In Bake options, choose a directory below `Assets/Generated/Textures`, the
   output profile, and optionally a material/rock or riverbed assignment.
   Existing output requires `Replace Existing Set`. Import and assignment occur
   only after the baker writes a successful completion manifest and all required
   maps are present.

Standard Tab/Shift-Tab navigation works throughout the UI. Command/Ctrl+N,
Command/Ctrl+O, Command/Ctrl+S and Command/Ctrl+Shift+S provide document
shortcuts; F5 or Command/Ctrl+Enter requests a preview.

## Validation

From the repository root, build the release baker and run Unity's integration
validation:

```sh
cargo build --release --locked \
  --manifest-path island-rs/Cargo.toml \
  --bin island-texture-baker

/Applications/Unity/Hub/Editor/6000.5.6f1/Unity.app/Contents/MacOS/Unity \
  -batchmode -nographics \
  -projectPath "$PWD/island-unity" \
  -executeMethod ProceduralMaterialEditorValidation.BatchValidateProceduralMaterialStudio \
  -quit
```

This loads Rust schema metadata, validates both committed recipes, builds their
schema-driven forms, exercises JSON Pointer edits and Unity Undo, verifies
atomic save/reload and external-change detection, renders 64-pixel previews
through the release baker, inspects their maps/timings/completion manifests and
confirms previews remain outside `Assets`.

## Troubleshooting

- If Auto Preview is paused, rebuild the release baker or configure its absolute
  path in Bake options. Cargo fallback is manual by design.
- Validation errors are Rust-owned. Click the diagnostic to focus the relevant
  visible control; a property hidden by the selected variant is still reported
  with its JSON Pointer.
- A preview never requires `AssetDatabase.Refresh`. If files appear under
  `Assets`, cancel and inspect the selected bake output path.
- Material assignment requires the `motu_unity_terrain` profile and a complete
  albedo, normal and packed-mask set.

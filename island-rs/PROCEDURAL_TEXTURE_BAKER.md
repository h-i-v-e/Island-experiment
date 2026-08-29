# Engine-neutral procedural texture baker

The `island-texture-baker` binary loads the current JSON recipe document,
evaluates it through `island-rs`, and writes a completed texture set:

~~~sh
cargo run --release \
  --manifest-path island-rs/Cargo.toml \
  --bin island-texture-baker -- \
  --recipe island-rs/texture-recipes/cracked-stone.json \
  --output island-unity/Assets/Generated/Textures/CrackedStone \
  --profile motu_unity_terrain \
  --normal-convention direct-x
~~~

separate writes albedo, Gray16 height, tangent normal, and Gray8 occlusion
PNGs. motu_unity_terrain additionally writes the packed RGBA mask
(R=height, G=occlusion, B=0, A=255).

## Current recipe format

Rust owns one current recipe shape. Unity and other engine integrations
edit the JSON document and send it back to Rust for validation and rendering;
they do not duplicate the noise or material evaluator. The root `material`
object remains the specialised base-height model and `albedo` remains the base
colour pass. Additional ordered layers are stored in `layers`:

```json
{
  "material": { "kind": "cracked_stone" },
  "layers": [
    {
      "id": "broad-colour-and-relief",
      "name": "Broad variation",
      "enabled": true,
      "source": {
        "kind": "fbm",
        "frequency": 3,
        "octaves": 3,
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
          "blend": "mix",
          "strength": 0.2,
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

`source` produces a normalised scalar. Its frequency, fractal controls,
offset, seed domain, cellular settings and optional explicit domain warp are
engine-neutral. `remap` is applied once before routing. A layer can contribute
to height, albedo, both or neither; height strength is expressed in metres,
while albedo strength is unitless. Layers are evaluated in order, and a mask
may only refer to an earlier layer by stable `id`.

The committed recipes are canonical examples. `cracked-stone.json` and
`rounded-river-stones.json` retain their converted baseline appearances;
`Bark.json` and `PlateBark.json` demonstrate editable tree-bark treatments.
The plate-bark variant combines vertically elongated fissured slabs, rough
faces and layered lichen without introducing an engine-specific recipe type.

## Runtime colour parameters

Recipes can declare typed linear-RGB parameters and bind albedo colours to
them. A binding may carry an authored `base` colour: its declared default then
reproduces that colour exactly, while an override tints it relative to the
default so gradients keep their internal variation.

The library call `bake_island_materials` accepts explicit engine-owned dirt and
stone colours, evaluates the approved embedded rock, river-bed, forest-floor,
and fallen-stone recipes, and returns owned maps without filesystem I/O. It
does not derive palettes from island seeds; Unity and Bevy own that policy.

## Editor protocol

The material editor uses the same baker for schema, validation and previews:

```sh
island-texture-baker schema --json
island-texture-baker validate \
  --recipe island-rs/texture-recipes/cracked-stone.json --json
island-texture-baker preview \
  --recipe island-rs/texture-recipes/cracked-stone.json \
  --output /tmp/procedural-material-preview --size 256 \
  --normal-convention open-gl
```

Normal convention is intentionally not recipe state. Each bake or preview
caller must request `open-gl` or `direct-x`; the generated manifest records the
choice so an engine can verify what it received.

Use `--parameters <FILE>` for a typed JSON override map or repeat
`--set-colour stone_colour=#77736d` for sRGB hex input. Hex values are converted
to linear RGB before the recipe is resolved.

These commands return machine-readable JSON on standard output. Human
progress belongs on standard error. Preview overrides only the requested
resolution and emits the same height, albedo, normal and occlusion maps used
by a final bake, plus selected-layer diagnostics; preview files stay outside
`Assets` until an explicit bake.

The output directory is created when needed. Existing files and unrelated files
are never replaced implicitly; pass --force to replace an existing generated
set. A forced replacement preserves `.meta` sidecars belonging to the exact
generated filenames, while unrelated sidecars still block the operation. Every
output is written through a temporary sibling and renamed, and the manifest is
written last. If a forced run is interrupted, remove any partial generated
files or rerun with --force; the previous manifest is invalidated before map
replacement so it cannot falsely mark a partial set complete.

For interactive authoring, run the standalone Bevy
[`Island Material Studio`](../island-material-studio/). It edits the portable
JSON recipes and calls this crate's validator, preview evaluator, and
transactional output writer directly in process. Unity and the Bevy island
viewer instead request their runtime maps in memory; Unity no longer contains
a separate recipe editor.

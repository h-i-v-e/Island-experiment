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

These commands return machine-readable JSON on standard output. Human
progress belongs on standard error. Preview overrides only the requested
resolution and emits the same height, albedo, normal and occlusion maps used
by a final bake, plus selected-layer diagnostics; preview files stay outside
`Assets` until an explicit bake.

The output directory is created when needed. Existing files and unrelated files
are never replaced implicitly; pass --force to replace an existing generated
set. Every output is written through a temporary sibling and renamed, and the
manifest is written last. If a forced run is interrupted, remove any partial
generated files or rerun with --force; the previous manifest is invalidated
before map replacement so it cannot falsely mark a partial set complete.

For local Unity authoring, build the release baker and open
`Island > Terrain > Procedural Material Studio`. The studio edits the portable
JSON recipe, validates saves through Rust, keeps previews under `Library`, and
imports assets only after an explicit successful bake. Cargo fallback remains
available for manual development previews and bakes, but auto-preview requires
the release executable. See the Unity
[`Procedural Material Studio` workflow](../island-unity/PROCEDURAL_MATERIAL_STUDIO.md)
for the complete artist-facing process.

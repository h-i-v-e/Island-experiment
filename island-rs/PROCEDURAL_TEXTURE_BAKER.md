# Procedural texture baker

The island-texture-baker binary loads a versioned JSON recipe, generates the
maps through island-rs, and writes a completed texture set:

~~~sh
cargo run --release \
  --manifest-path island-rs/Cargo.toml \
  --bin island-texture-baker -- \
  --recipe island-rs/texture-recipes/cracked-stone.json \
  --output island-unity/Assets/Generated/Textures/CrackedStone \
  --profile motu_unity_terrain
~~~

separate writes albedo, Gray16 height, tangent normal, and Gray8 occlusion
PNGs. motu_unity_terrain additionally writes the packed RGBA mask
(R=height, G=occlusion, B=0, A=255).

The output directory is created when needed. Existing files and unrelated files
are never replaced implicitly; pass --force to replace an existing generated
set. Every output is written through a temporary sibling and renamed, and the
manifest is written last. If a forced run is interrupted, remove any partial
generated files or rerun with --force; the previous manifest is invalidated
before map replacement so it cannot falsely mark a partial set complete.

For local Unity development, open Island > Terrain > Bake Procedural Textures,
select the recipe and an output folder under Assets/Generated/Textures, then
enable Use Cargo Fallback. A release island-texture-baker executable can be
selected instead. Unity refreshes and configures imported textures only after a
successful process exit.

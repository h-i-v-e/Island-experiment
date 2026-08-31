# Dynamic Cloud System Implementation Plan

## Goal

Add deterministic, dynamically configurable clouds that:

- move coherently across the island;
- obscure the sun, its sunset halo, and the moon;
- cast matching moving shadows over terrain, vegetation, rocks, rivers, and sea;
- appear consistently in planar water reflections;
- work across terrain and forest LODs without regenerating the island when ordinary cloud settings change;
- keep the portable weather-field generation in `island-rs` while isolating Unity-specific rendering in shared shaders.

The first version will be a 2.5D horizontal cloud layer. It will use ray/plane projection and a packed weather field rather than expensive volumetric ray marching. This preserves alignment between visible clouds and their shadows while keeping the receiver cost to one texture sample.

## Architecture

### Portable weather field

`island-rs` will generate one seamless RGBA8 weather texture from a seed and a power-of-two resolution:

- red: broad cloud masses;
- green: medium-scale structure;
- blue: fine erosion/detail;
- alpha: regional coverage modulation.

The weather map is static after generation. Coverage, density, wind, altitude, colour, and shadow strength remain runtime shader parameters, so changing them does not rebake the texture.

The Rust API owns an allocated `Vec<u8>` until the paired release function is called. Unity copies the bytes into a repeat-wrapped linear `Texture2D` and immediately releases the Rust allocation.

### Shared cloud field

`CloudCommon.cginc` will be the sole definition of:

- world-to-island-local coordinate conversion;
- wind animation and periodic coordinate wrapping;
- RGBA weather-channel combination;
- coverage-to-density conversion;
- optical transmittance;
- ray intersection with the cloud plane;
- low-elevation projection limits and shadow fading.

The sky and all shadow receivers must use this shared calculation. A cloud seen over the sun must therefore correspond to the same cloud field that attenuates sunlight on the island.

### Visible sky clouds

The existing sky dome remains the only geometry. For every sky fragment, the shader intersects the camera ray with a horizontal cloud plane in island-local space and samples the weather field at the intersection.

Rendering order inside the sky fragment will be:

1. atmospheric horizon/zenith gradient;
2. sunset halo and celestial discs;
3. cloud absorption over the background and discs;
4. cloud-scattered daylight or moonlight;
5. final night exposure.

This naturally obscures the sun and moon instead of adding an unrelated mask. Dense clouds become opaque; thin clouds transmit and tint the discs.

### Landscape cloud shadows

For a surface at position `P`, trace toward the active celestial source until the ray reaches the cloud altitude. Sample the same weather field there and convert density to direct-light transmittance.

Cloud transmittance will multiply:

- direct diffuse lighting;
- direct specular and wet highlights;
- sun/moon glints on water.

Ambient illumination will only receive a weaker configurable reduction so shadows remain soft and believable. Existing geometric shadow maps remain unchanged. Cloud sampling is not added to shadow-caster passes.

At night the active cloud-shadow direction changes to the moon only when the existing moon directional light is active. With no direct celestial light, receiver transmittance returns one.

## Runtime configuration

Add a serializable `IslandCloudSettings` section with live-clamped properties:

- enabled;
- seed;
- weather-map resolution;
- coverage;
- density/optical depth;
- altitude in metres;
- horizontal world repeat size;
- detail and erosion strength;
- wind direction and metres-per-second speed;
- day, sunset, and night cloud colours;
- direct-light shadow strength;
- ambient-light shadow strength;
- celestial obscuration strength;
- low-elevation shadow fade.

Changing the seed or resolution recreates the weather texture. All other values update global shader parameters live. Wind time is accumulated in a bounded offset to prevent long-running floating-point drift.

## Texture and sampler budget

The implementation adds exactly one repeat-wrapped RGBA weather texture. Each ordinary receiver samples it once. The sky may take nearby samples for optional edge lighting, but those samples use the same sampler.

Before enabling the system by default, Unity shader compilation on Metal must confirm that terrain remains within its texture/sampler limit. If the extra sampler exceeds the limit, the fallback is to place the weather channels in a shared environmental-noise texture already bound by the terrain rather than removing cloud alignment.

## Implementation stages

### Stage 1: Rust domain API and FFI

1. Add `clouds.rs` with validated options and deterministic seamless generation.
2. Test byte size, channel variation, seed determinism, seed differentiation, and edge continuity.
3. Export `CreateCloudWeatherMap` and `ReleaseCloudWeatherMap` through `ffi.rs`.
4. Extend the allocation-pair smoke test.
5. Add matching C# interop structs and declarations.

### Stage 2: Unity settings and lifecycle

1. Add `IslandCloudSettings` to `IslandSettings.cs` and expose it from `IslandGenerator`.
2. Create the native weather map once during runtime resource setup.
3. Upload it as a linear RGBA32 texture with repeat wrapping and mipmaps.
4. Destroy and regenerate it only when seed or resolution changes.
5. Release the Unity texture during generator cleanup.

### Stage 3: Shared shader contract

1. Add `CloudCommon.cginc`.
2. Publish the cloud texture and scalar/vector parameters globally.
3. Publish the island world-to-local matrix and active sun/moon source direction.
4. Add debug helpers for density and transmittance visualization.

### Stage 4: Sky and celestial occlusion

1. Extend `SkyDome.shader` with camera-ray/cloud-plane projection.
2. Composite lit clouds over the gradient and celestial bodies.
3. Attenuate the sun disc, red halo, moon surface, and moon phase consistently.
4. Fade or clamp projections near the horizon to avoid enormous unstable coordinates.

### Stage 5: Surface receivers

Apply direct-light cloud attenuation to:

- terrain LOD0, LOD1, and LOD2;
- grass and fur passes;
- tree wood, LOD0 foliage, and distant foliage;
- rocks;
- reeds and ferns;
- river and sea lighting, reflections, and glints;
- the simplified planar-reflection replacement shader.

Shared include points will be preferred so repeated multi-pass shaders do not acquire divergent cloud logic.

### Stage 6: Atmosphere and reflection integration

1. Ensure planar reflections render the clouded sky dome.
2. Apply the same receiver shadow approximation in the simplified reflection path.
3. Modestly reduce ambient light under broad overcast without double-darkening direct light.
4. Keep distance-haze colour controlled by the existing atmospheric system; clouds affect its brightness only through an explicit configurable overcast factor.

### Stage 7: Validation and tuning

Rust validation:

- `cargo fmt --check`;
- focused cloud unit tests;
- full `cargo test --all-targets`;
- `cargo clippy --all-targets --all-features -- -D warnings` if the existing baseline permits it.

Unity validation:

- compile runtime C# and validation assembly;
- import all shader variants on Metal;
- confirm no texture/sampler-limit failures;
- confirm no material displays the pink error shader;
- verify cloud settings update without island regeneration.

Visual acceptance scenarios:

1. Coverage zero produces a cloud-free result and no cloud shadows.
2. A dense cloud crossing the sun obscures both disc and halo while its projected shadow crosses the expected ground area.
3. At night the same behaviour uses the moon only while moon lighting is enabled.
4. Clouds and their shadows move together under wind and wrap without a visible seam.
5. Terrain LOD changes do not create holes or discontinuities in cloud shadows.
6. Water glints disappear beneath cloud cover and visible clouds appear in planar reflections.
7. Sunrise and sunset do not cause stretched, flickering, or inverted shadows.
8. First-person and overview cameras retain the existing sky, haze, and reflection behaviour.

## Performance guardrails

- one packed weather texture and one receiver sample;
- no allocations or texture regeneration during ordinary per-frame updates;
- no cloud work in shadow-caster passes;
- bounded animation coordinates;
- optional nearby density samples only in the sky pass;
- profile main and reflection cameras separately before and after integration;
- retain a single `enabled` branch that makes disabled clouds visually neutral.

## Completion criteria

The feature is complete when the portable field, FFI ownership, live Unity settings, visible sky clouds, celestial occlusion, receiver shadows, water/reflection integration, cleanup, and automated validation all pass, with unrelated working-tree changes left untouched.

# Ocean crest translucency — 10 September 2026

Added tinted, directional scattering to the sea water body before reflection,
foam, fog and the existing depth/refraction composition. Crest selection now
uses signed curvature of the combined wave field, rather than height above the
sea plane. Distance fade and ship suppression still apply. The
contribution uses actual main-light colour, shadow attenuation and cloud
transmittance. It has no fixed or ambient crest fill.

For the initial implementation, five focused Metal/Unity EditMode tests passed, with no skips: new rendered
translucency test, shared weather/waves, shore breaking, wave transitions and the
ship-deck GPU clamp. The new test compares strength zero against the default
under front/back lighting, tests flat-water and zero-light behaviour, and checks
finite GPU output. Ocean translucency: backlit=0.05397, frontlit=0.01256

Inspected the paired images from a controlled two-wave scene under identical
lighting with reflections enabled and foam disabled. Crests gain a blue-green
scattered-light gradient; troughs retain their deep-water colour. This is an
isolated rendering comparison, not validation in the user's running scene.

The test saves/restores shader globals and uses static phases. It does not change
wave simulation, buoyancy, geometry, refraction or the native plugin. No extra
texture fetches or render passes are introduced by the scattering term.

Controls and runtime material property names are documented in
[the ocean notes](../../OCEAN_WAVE_SYSTEM_PLAN.md#crest-translucency-10-september-2026).
Strength zero restores previous shading. The shader should reimport in Unity;
no native-plugin restart or island cache rebuild is required.


## Reduced strength and stronger view dependence

Removed the ambient and fixed sunlight contributions which kept crests tinted
from every direction. Reduced default strength from 1.25 to 0.4 and increased
sun directionality from 4 to 8. The grazing-face term now falls off quadratically
with viewing incidence, with no minimum. Restart Play mode to recreate runtime
materials with the new defaults; explicit material overrides retain their values.

The updated focused GPU rendering test passed. With ambient light present, its
average green-channel scattering contribution is 0.00269 in the backlit view,
0.00002 after moving the camera sideways, and effectively zero from above or
with front lighting. It also verifies bounded default intensity and zero added
scattering on a flat sea and with the main light off. The new test moves the
camera while keeping the sun fixed, as well as testing the reversed sun.

The current before/after images were inspected. The first stronger rendering is
retained as `ocean-initial-strong.png`. These are controlled comparisons; the
user's running scene still needs visual acceptance. Existing wave checks were
not rerun because wave geometry and its inputs did not change in this adjustment.


## Curvature-based crest selection

The current mask uses each band's negative second height derivative, reusing its
phase, wavelength, noise-scaled amplitude and choppiness. Contributions combine
with their signs before selecting positive convex curvature; outgoing/incoming
wave patterns blend through the same transition. Compressed shore breakers have
their own analytic second derivative. Combined curvature is adjusted for surface
slope, coastal depth, geometric fade and ship suppression before an exponential
response maps it to the scattering contribution.

This replaces the common height threshold. Small local crests can scatter while
riding in a larger trough below sea level. Sharper crests respond more strongly.
The new material control is `_WaveTranslucencyCurvatureScale`, default 3 metres;
it replaces `_WaveTranslucencyHeight`. These checks used strength 0.4 and directionality 8.
Curvature treats noise amplitude/warp and sampled depth as locally constant,
matching the existing slope model; it is an optical thickness approximation.

Eight focused Metal/Unity tests passed without skips. New GPU tests compare
analytic curvature against finite differences of rendered heights, exercise
increased choppiness, cancel opposite transition phases, distinguish a submerged
local crest from a true trough, and check the compressed shore-breaker face.
Existing shared-weather, transition, shore-breaking, deck-clamp and ship-texture
GPU checks passed. The camera-directionality and no-light/no-wave tests also
passed (average green contribution: backlit 0.00142, side 0.00001, front/overhead
approximately zero). The current before/after screenshots were inspected; the
prior height-mask result is retained as `ocean-height-mask.png`.

Only shading consumes the extra derivatives; displacement/physics wrappers
retain their outputs and let the compiler discard unused curvature work.
No native plugin rebuild or island regeneration is required. Restart Play mode
if a generated material has retained older property defaults. The live user
scene has not been visually checked, and no commit or push was requested.

Subsequent visibility tuning doubles the default strength to 0.8. Curvature,
directionality and all lighting equations are unchanged. The recorded captures
and measurements above use the previous 0.4 setting; this scalar-only adjustment
was checked in source without repeating the GPU tests.


## Stronger visibility default

Raised default strength from 0.8 to 4 (five times the preceding setting), with
slider range expanded to 0–8. Curvature, tint and directionality remain the same.
All three focused translucency/curvature GPU tests passed. Ocean translucency: backlit=0.01423, frontlit=0.00000, side=0.00012, overhead=0.00000
Inspected `ocean-strength-4.png`; crest tint is now clearly stronger in the
controlled backlit scene. Restart Play mode to pick up the default, or set the
live sea material's Wave Translucency Strength to 4.

Default strength subsequently increased from 4 to 8. The rendering test no longer
enforces the earlier subdued-intensity ceiling; it still checks a visible
backlit contribution and suppression from other viewing directions. Earlier
captures and measurements retain their stated strength settings.

## Broader sunlight and subtle ambient fill

Default strength is now 6, sun directionality 4 (previously 8), and the new
Translucency Ambient Contribution is 0.03. Ambient light shares the curvature
and grazing-face masks and respects cloud ambient attenuation.

All three focused Metal/Unity tests passed without skips (`unity-ambient-tests.xml`).
Average green contribution: backlit 0.04497, frontlit/ambient-only 0.00046,
side 0.00367, overhead 0.00016. At the same oblique camera position, directionality
4 contributes 0.03315 versus 0.01427 at directionality 8. Tests also verify zero
contribution without illumination or geometric waves and independently disabling
ambient scattering. Inspected `ocean-strength-6-ambient.png`; the brightening
follows individual crests. The live user scene has not been visually checked.

## Amplitude-weighted wave contributions

Each wave now provides a bounded phase/crest-shape response, weighted by its
actual amplitude and divided by the sum of amplitudes. This removes the raw
curvature inverse-square wavelength boost. Small waves remain proportionally
faint even when sharply curved inside a large swell trough. Shore breakers
join the same weighted sum, with weather transitions interpolating contributions.
The new Crest Response property defaults to 0.8 per metre and replaces combined
Curvature Response; strength 6, directionality 4 and ambient 0.03 are unchanged.

Nine focused Unity/Metal tests passed with no skips (`unity-weighted-tests.xml`),
covering raw derivatives, the weighted sum, trough ripples, wavelength independence,
transition interpolation, lighting, shore breaking and ship/deck suppression.
The 0.3 m ripple in a 2 m swell trough contributes under 5% of their combined
crest response. Lighting measurements: backlit 0.05017, frontlit/ambient 0.00052,
side 0.00402, overhead 0.00019. Inspected `ocean-weighted-crests.png`; this remains
a controlled render, not live user-scene validation.

## Feathered crest boundaries

Added a smoothstep feather over each wave's normalized crest shape before its
amplitude-weighted contribution. The dark edge now starts with a flat tangent;
tall waves cannot shrink the transition through rapid exponential saturation.
The full crest peak, overall strength 6, sunlight lobe and ambient fill remain.
The same feather applies to the shore breaker.

Five focused translucency GPU tests passed (`unity-feather-tests.xml`), including
soft boundary behaviour on 1 m and 20 m components, unchanged peak response,
amplitude weighting, analytical derivatives, and lighting directionality. The
controlled render `ocean-feathered-crests.png` was inspected. The user's live
scene and reported tartan appearance still need visual confirmation in Play mode.

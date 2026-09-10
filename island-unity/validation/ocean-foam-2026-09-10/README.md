# Single-pattern ocean foam validation

Unity 6000.5.6f1, Metal, isolated scratch project.

The initial run passed nine checks; the new foam probe failed an overly strict
coverage comparison at the animation wrap (about 0.0002 coverage difference).
The final foam-only run passed after testing UV continuity to 0.0001 and allowing
less than one 8-bit colour level of texture-filtering difference at coverage edges.
No production shader changes were needed between these runs. Ten distinct focused
checks passed in total. Source diff whitespace check passed.

The foam probe verifies visible deformation and animation, coherent translation,
continuous phase wrap, and disabling deformation independently of drift. Existing
transition tests verify 15% primary crest drift and distortion phase integration
through speed changes, and calm-weather freezing. Other checks cover translucency,
weather bindings, shore breakers, and ship/deck wave suppression.

Mean coverage change from distortion: 0.460; from advancing phase: 0.444.
The inspected PNGs show the same synthetic periodic texture undistorted, distorted
at phase zero, and distorted at phase 1.2. They deliberately expose stretching
and deformation; they are not screenshots of the user's ocean scene.

Defaults: drift fraction 0.15, distortion strength 0.65 texture tiles, distortion
frequency scale 0.32, distortion phase speed 0.65 radians/sec at reference wind.
No live Unity scene was restarted or saved. No commit/push was requested.

## Cellular distortion

Replaced the nested sine warp with smoothly weighted attraction to irregular,
animated cell sites. A compact 3x3 neighbourhood keeps the UV field continuous
as sampling crosses cell boundaries. Coverage still samples one texture; drift,
distortion strength/scale/speed, crest eligibility and translucency are unchanged.
This adds per-pixel cell arithmetic; live scene performance has not been measured.

Three focused tests passed (foam animation/continuity, shore breaking, translucency
lighting). A subsequent higher-resolution foam-only run also passed. Inspected
512-pixel captures using the actual generated weather noise: rounded cell pockets
with stretched detail between them. Synthetic probe captures remain available for
comparison. Captures are flat coverage probes, not live sea-scene renders.

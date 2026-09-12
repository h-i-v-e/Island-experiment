# Distant translucency and calm-sea foam — 2026-09-12

Unity 6000.5.6f1, Metal, isolated project: **22 tests passed** (`results.xml`).

- The actual sea shader retains measurable backlit translucency after geometric displacement fades to a flat plane. `distant-translucency.png` is a controlled single-wave fixture with reflections/whitecaps disabled; it is not a generated-island screenshot.
- GPU probe checks keep near-viewer hull-clamped crests dark and retain analytic distant crest response.
- Ambient foam is gated by local wave amplitude (zero at/below 3 cm, full by 10 cm) and river allowance (zero at/below 0.02, full by 0.10). A fixture prefilled with foam verifies that calm and strongly suppressed river regions clear stored patches as well as rejecting new whitecaps.
- Active seas retain foam history; existing decay, advection, reprojection and teleport checks pass.
- Ocean optics, coastal crossfade, translucency, foam, ship-wave and GPU buoyancy checks pass. Shader rendering and `git diff --check` passed for changed files.

The generated island scene still needs visual tuning. No native plugin changes or restart are required for these shader changes.

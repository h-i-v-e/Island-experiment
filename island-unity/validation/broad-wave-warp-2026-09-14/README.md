# Broad ocean domain warp — 2026-09-14

Previous fixes were committed and pushed as
10c4785ea9a81bdef97efa8241d7262d46b7b737. This broader-wave change is separate.

The old warp sampled the 64-cell red/green noise channels at 2048 m and
0.37 times that repeat: approximately 32 m and 12 m features. The new warp
uses the eight-cell blue channel for its main two axes, rotating and scaling
the second lookup to avoid matching repeats. At the existing 2048 m repeat,
features span approximately 256 m and 420 m. A small red/green contribution
retains fine variation. Texture fetch count stays at two per wave-field warp.

The current profile and defaults change from 9 m to 32 m warp strength; the
existing control allows 0-128 m. Noise World Size controls the breadth too.
Wave equations, per-band amplitude variation, authored directions/speeds,
and coastal wave blending are unchanged. Sampling stays in wind-advected
world coordinates and is shared by geometry, normals, foam and water queries.
Existing unrelated scene, weather driver, island factory and wave-height edits
remain local and were preserved.

Unity 6000.5.6f1 / Metal, isolated project /tmp/motu-ship-spray.7RcBx7.
Results: 17 passed, 0 failed.

The new GPU regression measured mean warp changes of 0.426 m over 4 m versus
16.684 m over 256 m. It checks broad rather than rapid local bending, translated
world/wind anchoring, the zero-strength opt-out, and decorrelation between
successive 48 m crests. Existing wave transition, shared-weather, buoyancy,
underwater and foam suppression tests passed. These establish source/GPU
behaviour; distant live-scene appearance still needs visual tuning.

Filter: OceanDomainWarpTests|WaveTransitions|WeatherAndWaves|OceanBuoyancyTests|OceanUnderwaterTests|ExistingFoamSurvivesCrestsSeaLevelAndTroughs|CalmAndRiverSuppressedSeaRejectsNewAndStoredFoam

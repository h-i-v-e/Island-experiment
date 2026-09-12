# Coastal wave crossfade validation — 2026-09-12

Unity 6000.5.6f1 / Metal, isolated project: **14 tests passed**.

- Coastal and ordinary wave heights and normals share complementary weights. The GPU fixture tests pure coastal waves, partial influence, 5/7.5/10 m depths, zero/half/full distance influence, disabled coastal waves and zero coastal amplitude.
- A deliberately attenuated ordinary-wave mask verifies that active coastal blending restores full ordinary swell at full depth/outer distance, while disabled coastal waves preserve the original attenuation.
- Coastal direction composition retains full influence through the final 16 m and suppresses waves on dry land and in carved rivers. The outer distance fade remains 96–128 m.
- Existing protection checks cover large-wave seabed limits, geometry/normal agreement, shore direction, physical wavelength and independence from mask resolution/coverage.
- Ocean optics, foam, translucency and GPU buoyancy checks passed. A stale hull-envelope fixture was updated to the previously implemented 10 m mask range.

No native plugin change is needed. Restart Play mode to rebuild cached coastal masks before judging the near-shore crossfade. These are controlled GPU checks; the generated island scene has not been visually tuned in this change.

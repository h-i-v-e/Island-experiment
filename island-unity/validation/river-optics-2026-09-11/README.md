# River optics validation — 2026-09-11

Unity 6000.5.6f1, macOS Metal, isolated project copied from the working tree.
`results.xml`: **13 passed, 0 failed**, using:

```text
Motu.Editor.RiverSurfaceTests|Motu.Editor.OceanOpticsTests|Motu.Editor.OceanTranslucencyTests
```

Three new river tests verify:

- Soft normal detail moves in increasing downstream distance, follows rotated
  channels and vertical waterfall faces, and fades when unresolved. Foam moves
  with the same channel coordinates and increases on steep sections. Generated
  river noise includes mipmaps.
- Rendering the actual river shader over an opaque bed preserves shallow detail,
  attenuates RGB by depth, and reproduces the unobstructed bed when absorption,
  reflection and foam are disabled.
- A curved channel with a sloping rapid, banks and a protruding rock renders
  finite colours, with measurable contributions from flowing foam and ripples.

The three PNGs show this controlled river fixture with all effects, without
foam/bank waves, and without foam or fine detail. They were visually inspected.
The fixture is not a generated island or a full gameplay-scene acceptance test.
GPU probes check downstream movement separately from these static images.

Ocean optics and translucency regression tests passed after moving the shared
Fresnel, sun reflection, absorption and refraction functions to WaterOptics.cginc.
River flow uses its own UV coordinates and speeds, not ocean wind. Planar
reflections are restricted to flat water very near sea level; elevated rivers
use the sky fallback. River foam is flowing shader detail rather than a temporal
simulation. Native terrain/river generation, bank distances and waterfall
geometry were not changed.

`git diff --check` passed. No standalone-player build, performance benchmark or
interactive generated-island river review was run.

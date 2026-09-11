# Ocean optics validation — 2026-09-11

Unity 6000.5.6f1, macOS Metal, isolated project copied from the working tree.
`results.xml`: **25 passed, 0 failed**. Filter:

```text
Motu.Editor.OceanOpticsTests|Motu.Editor.OceanTranslucencyTests|Motu.Editor.OceanHullSprayTests|Motu.Editor.OceanFoamTests|Motu.Editor.OceanShipWaveTests
```

The five new optics tests cover:

- GPU RGB exponential absorption at known distances, zero absorption and water Fresnel at normal/grazing incidence.
- Calm versus windy ripple normals, filtering unresolved detail into roughness, and broadening the sun highlight.
- Foam deposition, persistence, exponential decay, reprojection after moving the grid, wind advection across texels, teleport reset and disposal.
- Rendering planar reflections in Play Mode, mipmap availability, reuse between frames and disabling the reflection fallback.
- Actual rendered foreground geometry with the refraction shader, using all four screen-offset directions. The unsafe control produces a displaced red silhouette; the depth-safe result rejects over 98% of it. A thin sampling fringe can remain at depth discontinuities.

The three PNGs show one horizontal-offset refraction case. Red is an opaque
object above the water; green is the submerged background. The deliberately
large test offset makes incorrect foreground sampling obvious. These are
controlled diagnostic images, not the appearance of the final ocean material.

Also reviewed `OpenSeaWorld` in the existing Unity editor in Play Mode: the
moving ship, spray, ocean surface detail, highlights and crest shading render
together. Restored the editor to Edit Mode afterwards. No scene edits were
saved. This was a visual smoke check, not a frame-time benchmark or exhaustive
weather/camera review. No standalone-player or non-Metal graphics validation
was run.

`git diff --check` passed. The existing wave displacement equations were not
modified by this change; the spray, ship-wave, foam and translucency regression
tests passed alongside the new optics tests.

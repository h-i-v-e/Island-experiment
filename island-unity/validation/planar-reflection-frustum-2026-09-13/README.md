# Planar reflection frustum regression — 2026-09-13

The original code reproduced the reported `Screen position out of view frustum`
error with a 16000 m far plane when the viewer was 0.07 m below a horizontal
sea plane. Its mirrored eye was exactly on the +0.07 m clipping plane, so the
oblique projection was singular. The baseline height/pitch sweep failed with
40 frustum errors, all at that height (`before.xml` and the archived fixture).

The camera-space clipping plane now keeps the mirrored eye at least 0.01 m
on the rejected side. This leaves normal above-water clipping unchanged. Below
the clipping plane, it raises the reflection cutoff above the mirrored eye
instead of allowing an inverted or singular near plane.

Validation used Unity 6000.5.6f1 on Metal in the isolated project
`/tmp/motu-ship-spray.7RcBx7`, with the repository Assets synced into it.

Five tests passed (`after.xml`):

- Four crossing regressions: simplified and full rendering, each with clip
  offsets of 0 and 0.07 m. Each sweeps nine heights, seven pitches, and two
  water-plane tilts; it checks render progression, an invertible projection,
  and restored culling state. After resurfacing, GPU pixel readback confirms
  visible red geometry above water and no blue geometry from below water.
- The existing reflection render, texture mipmap, frame reuse, and cleanup test.

Test filter:
`PlanarReflectionCrossesClipPlaneWithoutFrustumErrors|PlanarReflectionsRenderAndGenerateRoughnessMips`

No frustum errors occurred in the final run. The user's running scene was not
manually retested. No native plugin changes or rebuild were required.

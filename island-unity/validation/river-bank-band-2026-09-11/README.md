# Single wobbly river-bank band — 2026-09-11

One soft band replaces the repeating cross-bank waves. Following visual
feedback, increased its independent wobble amplitude to 0.3 m (previous maximum
0.042 m), animated two along-bank noise scales at different speeds, and increased
base travel speed from -0.07 to -0.35 m/s. The centre is kept far enough into the
water to preserve the complete inward/outward sweep. Width remains 0.12 m.

Halved band strength from 0.12 to 0.06 in shader defaults, IslandRiver material
and runtime material setup. Noise varies only along the bank, maintaining one
band. Pixel coverage softens distant edges; steep faces suppress the band
independently of waterfall whitewater.

Unity 6000.5.6f1 / Metal: all three RiverSurfaceTests passed. Inspected the
updated render included here: the band has clearly visible waviness with lower
opacity. No full generated-island visual review was run.
`git diff --check` passed.

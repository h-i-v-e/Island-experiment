# River waterfall flow and refraction restoration — 2026-09-11

The river makeover slowed/enlarged the waterfall noise and reduced refraction
offsets to the subtle lighting normals. Restore the original fine texture
repeat sizes (6 m broad, 3 m fine), with independent waterfall flow at 2.25 and
9 m/s. Blend this moving whitewater pattern onto steep faces while retaining
soft detail on calm reaches. Waterfall Flow Speed controls the fast band; the
broad companion uses one quarter of that speed.

Refraction now has its own moving noise offsets, with default Underwater
Distortion 0.012. Reflection distortion still uses the soft normals. The shared
foreground rejection and depth-dependent fade remain unchanged.

Unity 6000.5.6f1, Metal, isolated project: **8 passed, 0 failed** in
`RiverSurfaceTests` and `OceanOpticsTests`. GPU checks establish that the fine
waterfall pattern moves 0.9 m downstream in 0.1 seconds and changes rapidly.
The actual river shader visibly distorts a checkerboard bed with normal ripples
disabled, while distortion fades out at near-zero depth. The existing shared
foreground-refraction checks still pass.

Inspected the patterned-bed pair and river/rapids render included here. These
are controlled fixtures; no interactive generated-island acceptance or
performance benchmark was run. `git diff --check` passed.

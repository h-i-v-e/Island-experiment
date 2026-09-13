# Water viewport edge transitions — 2026-09-14

The screenshot shows sharp wedges at the water viewport edge. Both active
sea/river planar reflections and depth-safe refraction used binary texture
bounds tests. A distorted sample crossing those bounds abruptly switched
between the planar capture and sky, or between distorted and original UVs.

A shared smooth border weight now fades over the outer 8% of each texture.
Planar reflections blend to sky before leaving the capture. Refraction offsets
fade towards zero near viewport edges, and candidate sample validity also
fades instead of switching at the border. Foreground depth rejection remains.
The legacy shared water helpers use the same edge treatment.

Validation: Unity 6000.5.6f1 / Metal in /tmp/motu-ship-spray.7RcBx7.
The GPU reflection regression renders the production optics function with a
white reflection and black sky, testing all four borders with and without
ripple offsets. Before, the exact boundary still returned full reflection;
after, the falloff is continuous with no outside-edge streak and full interior
strength. Existing sea-depth, river and underwater rendering tests also pass.

Results: 12 passed, 0 failed (after.xml).

Filter: ReflectionsBlendAtEveryViewportEdgeInsteadOfSwitchingAbruptly|SeabedFadesBeforeMeshCutoffAtDifferentCameraAnglesAndSeaLevels|RiverSurfaceTests|OceanUnderwaterTests|PlanarReflectionsRenderAndGenerateRoughnessMips

The user's exact camera view was not recreated; that view still needs visual
confirmation. No native plugin changes were required.

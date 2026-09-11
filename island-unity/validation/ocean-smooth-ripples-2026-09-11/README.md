# Smoother animated ocean detail — 2026-09-11

The previous ripple bands shared one moving coordinate system, preserving an
embossed shape. Their high gradient gain and a separate dense reflection-noise
lookup made the surface resemble textured glass.

Reduced default ripple strength from 0.3 to 0.12 and increased wavelength from
0.55 m to 1.2 m. Smoothed the noise before differentiation, reduced gradient
gain and secondary-band weight, and gave the bands different travel speeds and
headings. Reflection/refraction distortion now follows these animated normals.
The new Fine Ripple Animation Speed control defaults to 1; zero freezes detail.

Unity 6000.5.6f1, macOS Metal, isolated project: **10 passed, 0 failed** in
`OceanOpticsTests` and `OceanTranslucencyTests`. The updated GPU test measures
gentle normals, a change after half a second of wind travel, frozen animation at
zero speed, and distant-detail filtering. Mean absolute XZ normal perturbation:
0.05735; mean change after half a second at 12 m/s wind: 0.05766. Absorption,
refraction, foam history, reflection mipmaps and translucency checks also pass.

The geometric wave equations are unchanged. This is a shading correction, not
a wave-height or buoyancy change. No standalone-player or performance benchmark
was run.

Reviewed the updated material in `OpenSeaWorld` after restarting Play Mode to
recreate the generated ocean following script/shader import. Broad waves are
visibly smoother, with softened moving surface highlights. Left the scene
running, matching its initial Play Mode state; no scene changes were saved.

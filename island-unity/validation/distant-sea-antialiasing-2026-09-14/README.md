# Distant sea antialiasing — 2026-09-14

The sea fragment shader now passes its world-space pixel footprint into a
filtered wave evaluation. Individual wave bands and crest harmonics fade
smoothly from eight to two pixels per cycle. Outgoing and incoming wave
patterns filter at their own wavelengths. Lost slope variance broadens the
highlight lobe instead of leaving flickering bright samples. Coastal variance
uses a phase-independent envelope so it cannot introduce its own flicker.

Foam uses footprint-selected noise mipmaps, with a wider coverage threshold
as the distribution averages out. Hull displacement foam uses that same
filtered coverage. The existing no-footprint overloads retain unfiltered
geometry, buoyancy queries and foam-history simulation. Nearby resolved wave
shading remains unchanged. This change extends the still-uncommitted broad
wave warp; no further commit or push was requested.

Validation: Unity 6000.5.6f1 / Metal in /tmp/motu-ship-spray.7RcBx7.

- Initial integration run: 23 passed. Includes antialiasing, broad warp, wave
  transitions, buoyancy, underwater rendering, foam suppression/persistence,
  and the cruising ship without an aft wake.
- Final run after making coastal unresolved variance phase-independent:
  11 passed, 0 failed; antialiasing, underwater and cruising scene renders.
- GPU wave cases cover 4, 16 and 64 m wavelengths, progressive filtering,
  preserved near detail, retained unresolved variance and unaltered physical
  queries. Mean foam change under a small sample shift fell from 0.11204 to
  0.00006 in the dedicated minification fixture.

An isolated scene preview was inspected. The user's distant camera view and
performance still need live-scene confirmation; no native rebuild is needed.

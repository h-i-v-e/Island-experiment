# Height translucency and boat displacement foam

Unity 6000.5.6f1, Metal, isolated scratch project: 11 tests passed, none skipped.

GPU checks cover displacement-proportional boat foam for hull pull-down and bow
wave raise, unchanged troughs and neutral/disabled fields, disabling foam strength,
height relative to the largest enabled wave peak, weather/noise/choppiness scaling,
soft sea-level fade, sunlight and ambient behaviour. Existing foam animation,
wave transitions, shore breaking, weather bindings and ship/deck tests also passed.

Inspected the controlled `ocean-height-translucency.png` render. Boat foam uses
the actual per-vertex difference from its original wave position, passed to the
fragment and textured with the cellular foam mask after hull suppression. Tests
probe the boat field and scalar response; the live ship scene was not rendered.

Defaults retained: translucency strength 6, sun directionality 4, ambient 0.03.
New Boat Displacement Foam gain: 1 per metre. No commit or push requested.

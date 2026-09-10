# Outward waterline blend

The hull ceiling previously used the raw weather amplitude even though rendered
waves are depth-limited to at most 5 m. This compressed the effective transition
into a narrow rim. Vertex displacement and shading now use the depth-limited
envelope. The upper-ceiling behavior remains: troughs below it stay untouched.

Generator defaults are clearance 0 m and outward blend 12 m. Regenerated through
the live Unity editor and saved Pirate Ship High Poly Waterline 2.png, assigned
in OpenSeaWorld with dimensions 43.80256 x 58.709187 m, scale 1, runtime feather 0,
bow pullback 12 m and tip blend 2 m. Texture resolution is 382 x 512. The main
project PNG exactly matches the isolated actual-ship bake (SHA-256
1d8e9b75493c9465978901741e5a941bcea21907069e1cb058ffede066a03616).

Eleven distinct checks passed in Unity 6000.5.6f1 with Metal: ten in the initial
focused run and the actual-ship bake on rerun. The initial bake test failed because
the scratch scene referenced a texture GUID that was absent from the scratch
assets; syncing the existing PNG and its meta fixed the fixture. Both result
files are retained. Checks cover the depth-limited GPU envelope at large and
small weather amplitudes, shallow water, outward mask falloff, generator geometry,
ship-following texture fields, foam displacement and crest-only clamping.

The scene assignment was saved and Play mode resumed. Final appearance from the
user's beside-hull viewpoint remains to be confirmed.

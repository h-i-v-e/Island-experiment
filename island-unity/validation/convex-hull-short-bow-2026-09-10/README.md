# Convex hull shoulder and compact bow

The authored clamp is now (1-t)^3, where t is outward distance across the
12 m band. Its height ceiling therefore rises as 1-(1-t)^3: a convex shoulder
that eases toward the surrounding ocean. Full clamping remains at the waterline,
and waves below the ceiling remain untouched.

Bow rise is independent data in the same PNG's alpha channel. Its ridge is
centred 1 m outside the actual silhouette, has 1.2 m width, and fades in over
the forward quarter of the hull. It no longer inherits the clamp blend distance.
The component flag preserves legacy greyscale textures and wake stamps. Generated
assignments enable alpha bow data, zero pullback, and a 1 m bow-tip taper.

Regenerated through the live Unity authoring tool and saved OpenSeaWorld with
Pirate Ship High Poly Waterline 3.png. Dimensions 350 x 512 pixels, mapped to
32.203545 x 47.109184 m. Main and isolated bake PNG hashes match:
505b2e8121bd637cef1acf53d2e0dd1625bf1ef7faf6fa956da6acadb28fbc71.
Verified linear import, alphaIsTransparency off, and saved component values.

Nine distinct checks passed in Unity 6000.5.6f1/Metal: eight generator checks and
one GPU test. The first GPU assertion incorrectly expected the full bow height
without its overlapping ceiling; corrected to expect the capped rise. One rerun
hit a Unity Mono named-pipe native crash before producing results; the subsequent
run passed. Tests cover convex slope, unchanged bow extent under wider clamping,
actual-ship baking, alpha overlap, legacy stamps, rotation, speed and wake fade.
Final running-scene appearance remains subject to visual tuning.

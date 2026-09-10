# Wider authored waterline mask

Six generator tests passed in isolated Unity 6000.5.6f1. The new test compares
2 m and 6 m bands and checks retained clamping farther outside the hull, monotonic
falloff and a bounded adjacent-sample gradient.

Regenerated the actual selected ship via Island > Waterline Texture Generator
in the user's Unity editor after leaving Play mode. Saved and assigned Pirate
Ship High Poly Waterline 1.png with 6 m edge blend, 1.5 m clearance, original
bow ridge parameters and 512-pixel longest edge. The authored map is 358 x 512
pixels, mapped over 34.7576 x 49.70919 metres. Its bytes match the isolated test bake.

Verified the scene references the new texture GUID and stores scale 1, runtime
feather 0, bow pullback 7.5 m and tip blend 2 m. Footprint endpoints and radius
remain unchanged. The user resumed Play mode; the resulting live view showed
foam around the forward hull. Overall appearance remains a tuning judgement.

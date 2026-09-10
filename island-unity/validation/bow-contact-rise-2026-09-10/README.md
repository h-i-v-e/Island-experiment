# Bow contact and outward rise

Four focused Unity 6000.5.6f1 Metal checks passed in the isolated scratch project.
The new GPU test uses a hull with a known bow tip and a raised ridge four metres
ahead. It verifies the ridge moves back, the contact point stays close to sea
level after field filtering, and the water rises outward through the tip taper.
The other checks cover hull footprint feathering, authored hull/bow/wake channels
(with reshaping disabled), and displacement foam including the height ceiling.

An initial cubic tip fade left about 9 cm of lift at the exact bow sample because
of the field's bilinear filtering. The final quintic fade passes the under-8-cm
contact check while preserving the rise farther out.

Defaults: hull clamp scale 1.0, feather 3 m, bow pullback 3.5 m, tip blend 2 m.
Changes operate on existing hull textures at stamp composition time. The clamp
mask and deposited wake are not translated. Live scene appearance and performance
have not been checked. No commit or push was requested for this tuning.

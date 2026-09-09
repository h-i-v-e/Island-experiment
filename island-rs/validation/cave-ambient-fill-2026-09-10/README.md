# Cave ambient fill — 2026-09-10

Added a normal-independent minimum ambient contribution to the cave shader's
base pass, fading out with the existing entrance ambient mask. Default 0.12,
range 0–0.5. Direct shadows and additional light passes retain their equations.
CaveStreamer.InteriorAmbientFill updates the generated material immediately;
the cave inspector exposes the serialized field. Zero restores prior darkness.
The setting is per live cave set and resets on island regeneration.

All 29 isolated Unity cave/runtime tests passed, including the runtime material
control and decorated-branch traversal at origin and translated 18 km. The shader
compiled and a torch-lit junction render was inspected against the previous run.
Sample red-channel levels in shadowed areas increased from 2 to 7, 4 to 9,
21 to 31 and 34 to 43 (8-bit capture), confirming that the fill lifts dark surfaces.
The live user scene was not rechecked. No native plugin, snapshot version or cave
algorithm revision changed for this adjustment. No commit or push was performed.

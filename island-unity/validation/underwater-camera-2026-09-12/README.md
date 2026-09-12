# Ocean underwater camera

Previous water work was committed and pushed as `7e2a2f7d4a47385a179c94dd27b946357ced1cfc` before this implementation.

The camera effect uses a 32 x 32 signed wave-height map at the near plane and a full-resolution interface-depth pass over the actual ocean mesh. Both sample the shared wave, coastal, and hull-clamp equations. Submerged pixels receive distance-based RGB absorption and haze; opaque geometry and the first water exit limit the path length. The sea renders its underside with water-to-air refraction and an approximate water-colour reflection beyond the critical angle. Independent elevated rivers are not underwater volumes.

World initialization attaches the effect to existing reflection-equipped game cameras; additional game cameras are bound before rendering. Reflection cameras are excluded. Bridge/exploration switching preserves effect activation. Tunable haze, visibility and waterline softness are on Ocean Underwater View.

Validation: isolated Unity 6000.5.6f1 project, Metal, 34/34 selected checks passed. These include six new underwater cases (level and rolled split views, orthographic split view, surface/foreground path termination, rolled exit-mask alignment, and wave animation plus unchanged above-water colour). The cruising-ship runtime check confirms automatic attachment to the active camera. Existing ocean optics, translucency, foam, ship effects, rivers and runtime/scene contracts also pass. The final log has no shader/C# errors or image-effect destination warnings. Scoped whitespace checks pass.

The PNGs are controlled render fixtures with white incoming radiance, not screenshots of the user's active scene. Level/rolled partial views, the view up through the surface, and rolled exit-depth captures were inspected. Runtime scene appearance and performance still require interactive acceptance. Restart Play Mode to initialize the camera components in an already-running scene.

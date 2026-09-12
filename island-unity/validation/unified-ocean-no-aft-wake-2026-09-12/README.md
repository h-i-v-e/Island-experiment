# Unified ocean and disabled aft wake

Removed the per-island Coastal Water Overlay shader, plane, material, and lifecycle references. Shallow tint now runs on the displaced Sea Water surface using composed coastal depth and distance; tint fades out at 10 m depth and over 24 m at the composed mask bounds. Per-island masks and their registration remain required runtime resources.

OpenSeaWorld has no Wake Texture assigned, so it deposits no aft wake stamps. Hull waves, contact foam, and hull spray remain active. Optional wake authoring support remains available for other ships.

Validated in an isolated copy with Unity 6000.5.6f1 on Metal. The initial 29-test run passed 28 checks; the old cruising-ship test expected a trailing wake and failed with zero stamps. Updated that test to require no wake while retaining hull texture and spray, then reran it and runtime/mask ownership: both passed. Thus all 29 selected checks pass with the revised scene contract. Tests cover GPU optics, tint depth/distance/bounds, foam, ship effects, rivers, scene contracts, and native island generation/cancellation/unload. The runtime test verifies no coastal overlay object is created. Scoped diff whitespace checks pass; the scene's pre-existing whitespace and spray tuning were preserved.

This is controlled rendering and runtime validation, not visual acceptance in the user's active scene. Restart Play Mode to regenerate any already-loaded islands without their old overlay objects.

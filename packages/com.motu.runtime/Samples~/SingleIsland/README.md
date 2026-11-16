# Single island example

Create an empty GameObject, add `Motu > Single Island`, and assign your existing
camera/player as Streaming Target. Enter Play mode. The default 2 km island is
centred on that GameObject; use unit scale and Y-only rotation.

The component exposes all generation/material settings. It creates terrain and
vegetation without creating a camera, input controller, ocean, sun, or sky.
Use the runtime package README for an asynchronous script example and optional
navigation. An example does not need to overwrite your project's scene settings.

Alternatively add `SingleIslandExample` to an empty GameObject. Assign a viewer
and seed; it creates a child island when Play starts. Use this script as the
smallest example of integrating without importing any demonstration scene.

If the host uses `WorldEnvironmentController`, initialize that environment and
call `singleIsland.Generator.ConfigureEnvironment(environment)` before generating.
Set `GenerateOnStart` to false when the host coordinates asynchronous startup.
This optional binding registers the terrain's coastal mask with the existing
ocean, enables its shoreline wave field and land attenuation, and shares its
water lighting. Keep the environment on a separate GameObject: its follow target
moves the ocean anchor, while the generated terrain must stay fixed. Clearing or
regenerating the island unregisters the old mask; the host still owns the ocean.

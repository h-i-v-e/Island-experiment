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

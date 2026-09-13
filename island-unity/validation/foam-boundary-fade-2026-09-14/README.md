# Foam boundary fade — 2026-09-14

The shared 256 m foam history map previously faded only within 4% of its
square edge (10.24 m). It now fades radially from 64 to 128 m from its centre,
with smooth endpoints. Direct wake foam uses the same normalized transition
in its 512 m field (128 to 256 m). Hull clamping and wave displacement retain
their existing field-edge treatment. Foam deposition, decay, patch animation,
and calm-water suppression were not changed.

Unity 6000.5.6f1, Metal, isolated project `/tmp/motu-ship-spray.7RcBx7`.
The initial GPU regression failed against the original shader: stored foam at
96 m was still fully opaque instead of partway through a broad fade.

Final run: 10 passed, 1 failed. Passing coverage includes GPU fade continuity,
matching axial/diagonal falloff, translated fields, preserved hull/height
channels, patch animation, foam history, calm-water suppression, hull/bow
fields, and wake ageing/teleports.

The failure is `CruisingShipKeepsHullWavesWithoutAnAftWake`: the current scene
assigns `ShipWake`, while the existing test requires a null aft-wake texture.
The scene and ship configuration were preserved. This assertion does not
exercise the changed shader fade.

Filter: `OceanFoamTests|OceanShipWaveTests|CalmAndRiverSuppressedSeaRejectsNewAndStoredFoam|FoamPersistsDecaysReprojectsAndResetsOnTeleport`

Results are in before.xml and after.xml. No live-scene visual acceptance was
performed. No native plugin changes were needed.

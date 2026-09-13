# Foam height cutoff and disabled aft wake — 2026-09-14

The committed OpenSeaWorld scene still assigned ShipWake to the ship's
OceanDeckWaveClamp. That one assignment is now null; the other existing scene
edits are preserved. The cruising test confirms zero aft-wake stamps while
hull waves and hull spray remain enabled.

Stored foam was both hidden by the final rasterized height and cleared by a
binary history activity test driven by instantaneous wave height. This cut
through trails whenever a wave dropped below the crest-height threshold.

Foam persistence now uses the local wave amplitude envelope and river mask.
Fresh whitecaps retain their instantaneous and rasterized crest-height checks.
Existing foam can pass through sea level and troughs, with its usual timed
decay. The calm envelope blends over 0.10-0.25 m to preserve the small-wave
cutoff. History activity uses a smooth transition instead of a step.

Unity 6000.5.6f1 on Metal, isolated project /tmp/motu-ship-spray.7RcBx7:

- Before: the new history regression failed at wave phase -pi; old foam was
  cleared to zero instead of decaying to approximately 0.98347.
- After: all 12 tests passed. Coverage includes history and visible-surface
  foam through a full wave cycle, calm/river suppression, radial map fading,
  animation, persistence/advection/teleports, hull and bow fields, and cruising
  without an aft wake.
- The test-generated scene preview was inspected and is included here. This
  is a single isolated scene capture, not acceptance of the user's live view.

Filter: ExistingFoamSurvivesCrestsSeaLevelAndTroughs|OceanFoamTests|OceanShipWaveTests|CalmAndRiverSuppressedSeaRejectsNewAndStoredFoam|FoamPersistsDecaysReprojectsAndResetsOnTeleport

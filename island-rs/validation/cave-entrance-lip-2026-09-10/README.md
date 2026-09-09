# Entrance lip checkpoint — 2026-09-10

The previous cave work was committed and pushed as 98f8c90 before this change.
Cave revision 12 changes exterior lip geometry and normals; the native snapshot
format remains 10. Unity's revision key regenerates cached islands.

The exterior cubic's second control point now stays nearer the cliff, making
its initial inward turn more gradual. The outer terrain boundary and inner
throat positions and normals remain exact. Intermediate lighting normals are
perpendicular to the curve tangent. Exterior lips use 24 segments instead of 12;
interior geometry retains its existing curve and 12 segments.

Validation:

- 20 native cave tests passed, including crown tangent/normal continuity,
  preservation of the original roof, shared floor/wall edges, oblique joins,
  branching volume topology and walking clearance.
- Production Clippy passed with warnings denied.
- Five generated terrain-size-128 islands (seeds 0–4) completed successfully.
- The isolated Unity entrance test passed with the final exported geometry,
  including entering/exiting, restored exterior collision, walking above the
  roof and a rendered upper-lip view (upper-lip.png).
- The broader 29-case Unity suite passed during the earlier normal-only
  iteration; only the affected entrance test was repeated after widening the
  cubic bend. This is not a fresh standalone player-build check.
- Release native library rebuilt and atomically installed; SHA256 `be40761e93996eeb2bde1c6781e0b9617cf5ec9a2f1da93ca36e88547be7fb42`.

The fixture render was inspected. The user's live island was not restarted or
visually rechecked. Restart Unity to load the new native library and regenerate.
The entrance refinement remains uncommitted after the requested checkpoint.

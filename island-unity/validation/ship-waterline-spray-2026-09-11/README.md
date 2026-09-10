# Ship waterline spray

Implemented a closed 56-sample loop on the OpenSeaWorld pirate ship, authored from
its filled resting-waterline silhouette. Local points and the authored plane
normal survive the imported model's -90 degree rotation and 1578.65 scale. One
world-space particle system emits along wet portions of the connected segments,
with launch speed proportional to natural wave penetration, capped at 18 m/s.
Particles pass through the hull. Density integrates penetration over segment
length; a shared emission budget and live-particle cap bound output.

All 19 focused tests passed in an isolated Unity 6000.5.6f1 project with Metal:
OceanHullSprayTests, ShipWaterlineTextureTests, and OceanBuoyancyTests. These
include live asynchronous GPU sampling and particle simulation, 128-probe query
support, a crest preserved by queries while the rendered ship clamp suppresses
it, transformed/scaled hulls, dry sections, crest-to-trough teleports, component
re-enable, rate limits, and lowering the live particle budget. Geometry checks
cover concavity, closure, density under subdivision, and the saved ship loop.
The final XML is retained here. One intermediate check exposed Unity retaining
existing particles above a newly lowered limit; the component now trims those
particles when that setting changes, and the final rerun passed.

The real ship was configured and saved through the Unity editor. Play Mode Scene
view inspection showed white spray emerging from the hull and spreading upward
and outward around the bow as the hull moved through waves. This was a visual
smoke check, not a performance profile or a full weather/viewpoint tuning pass.
The editor was returned to Edit Mode with the ship's spray controls visible.

The isolated import reports the existing ProjectSettings/TagManager.asset parser
warning. It did not prevent compilation or these tests from passing; that
unrelated project setting was not changed. No standalone player build was run.

## Direct height gap and relative velocity update

Removed the spray height filter. Strength now uses the direct world-space gap:
`max(0, preClampWaveY - emitterY) * (1 + relativeVelocityInfluence * closingSpeed)`.
The same strength controls emission density and launch speed, with the existing
limits. The saved ship uses influence 0.25 per m/s; zero restores height-only
strength. Closing speed adds horizontal approach along the segment's outward
normal and upward water motion relative to the ship. The ship's point velocity
includes rotation. Separating or shared motion adds no boost; dry segments stay
inactive. Each segment uses its midpoint relative motion and retains the exact
linear clipping of its wet portion.

Velocity-enabled GPU queries return a second row with local water displacement
velocity. They use a centred 40 ms difference at the same undeformed water point,
advancing both phase banks, the onshore phase, and wind-advected noise. Current
weather and transition weights are fixed for the derivative. The motion is the
local water's velocity, not crest propagation speed or the difference between
heights at moving emitter positions. Height-only buoyancy keeps its existing
shader variant, output layout, and evaluation cost.

All 20 focused spray, buoyancy, and ship-wave checks passed on Unity 6000.5.6f1
with Metal; see velocity-tests.xml. New coverage verifies known horizontal and
vertical water velocities, zero-speed waves, weather phase-speed scaling,
matching height-only results, approaching/separating/shared/tangential motion,
and proportional height response. The live particle, dry/teleport/re-enable,
budget, and GPU clamp/wake regressions also passed. The earlier real-ship visual
smoke check predates this velocity update; no new visual tuning or profiling is
claimed for this revision.

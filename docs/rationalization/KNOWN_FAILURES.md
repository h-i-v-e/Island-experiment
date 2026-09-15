# Baseline reconciliation and fixture gaps

## Native generated-river integration fixture

`cargo test --manifest-path island-rs/Cargo.toml --locked --no-default-features
--test generation rivers_are_continuous_flowing_terrain_submeshes_with_waterfalls`
fails for its seed-666, 65-initial-point fixture. The generated river mesh has
zero vertices/triangles, so its required positive bank-distance UV cannot exist.
The same failure was reproduced with the starting native implementation, with
this change's ABI and material-selection additions removed. See
[baseline-river-failure.txt](baseline-river-failure.txt).

An isolated diagnostic using the same seed with the full default resolution did
produce river geometry (800 vertices, 1,296 triangles, 500 positive bank-distance
UVs). It then failed the existing centreline-clearance assertion on a submerged
node where bed height and surface were both -0.0010161372. This is evidence that
simply increasing the fixture size does not reconcile all its assumptions.

The fixture is now split around the existing contracts:

| Previous assumption | Current protection |
| --- | --- |
| Every generated river has visible water | Seed 666 at 65 points explicitly verifies submerged rivers remain queryable with an empty visible mesh. The default-resolution fixture separately requires visible water. |
| Continuity, ocean reachability, downhill flow and increasing discharge | Both generated fixtures call the same network assertions, including downstream sea handoff. |
| Bed clearance for every node, including ocean-only channels | Clearance is required for above-sea water surfaces. Submerged channels are covered by the sea handoff test rather than a separate water-surface requirement. |
| Valid indices, positive interior/zero boundary bank UV, sea clipping and confluences | Retained on the full-resolution generated mesh. |
| Multiple differently sized waterfalls and flat reaches in an incidental island | Existing prescribed gradient/profile tests check waterfall frequency, relative heights and terrace flatness. A new constructed channel exports three drops and flat reaches through the actual mesh pipeline. |
| Flow UV includes waterfall height | The exported multi-waterfall fixture checks each UV increment against the full 3D channel distance, using channel positions after refinement. |

The old original-XY lookup could miss channel vertices moved by refinement. The
constructed fixture uses the final terrain positions. No production river
algorithm, shader, quality setting or material changed to satisfy these tests.
The two generated fixtures passed together in release CPU tests (panic=unwind for
the test build); the new exported-waterfall unit test passed in debug CPU tests.
The complete debug CPU suite passed: 468 tests, zero failures and three existing
ignored tests. All 35 generation integration tests passed. Full output is in
[native-contract-suite.txt](native-contract-suite.txt).
Historical failure logs remain available; the test was not ignored.

## Resolved stale material expectations

The starting material CLI test failed its cracked-stone albedo hash. The earlier
authored colour-parameter change in commit `e5f0465` also changed river-stone
albedo; both old hashes were stale. Their expected albedo hashes now match the
existing authored defaults. All eight non-albedo hashes were retained. The six
material CLI tests pass; no recipe or image-generation algorithm changed here.

## External fixture gaps

Eight Unity cave tests were skipped because their separately generated snapshot,
network and walk fixtures were absent. They are not counted as passes. The
fixture environment variables and remaining performance/visual/release gates
remain listed in the implementation report and original plan.

## Resolved release-only forest reference assertion

A release CPU run exposed a two-ULP mismatch in the forest scale test's
constant-folded reference calculation. An isolated pre-change checkout with
identical forest/noise sources reproduced it. Runtime inputs via `black_box`
prevent folding of that reference; the exact bit assertion, range checks and
monotonicity checks are retained. Both debug and release pass. Forest generation
was not changed. Evidence is in the `hydraulic-optimization` directory.

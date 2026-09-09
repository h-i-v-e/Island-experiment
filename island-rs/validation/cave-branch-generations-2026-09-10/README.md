# Branch generation checkpoint — 2026-09-10

Cave algorithm revision 11; native snapshot format remains 10 and the 24-byte
walk options ABI is unchanged. Unity's revision key regenerates cached islands.

Each child doubles its parent's ending probability, capped at 1. Branch
probability is independently valid from 0 to 1; defaults are 0.25 branching and
0.09 main-passage ending. Siblings do not affect each other's or their parent's
ending chance. Returning paths stay behind the entrance blend plane, preventing
additional strips being pulled back to the mouth.

## Validation

- 19 native cave tests passed, including a 64-seed comparison of low and 100%
  branching. The test exercises recursive and terminal generations and ensures
  terminal branches have one segment and no descendants.
- Strict production Clippy passed with default features and without them.
- 20 complete terrain-size-128 islands passed with defaults: 18 caves, 90 total
  branches, maximum 28 branches/island and 97,839 triangles/island.
- Five complete terrain-size-128 islands passed at 50% branching: five caves,
  77 total branches, maximum 47 branches/island and 78,164 triangles/island.
- Isolated Unity run passed 27 of 29 cases; two traversal cases stopped at an old
  hard-coded two-branch assertion (the new fixture has eleven branches). After
  correcting the assertion to use fixture size, all four traversal cases passed,
  walking every branch both at origin and translated 18 km. All 29 unique cases
  have passed. The old junction-facing regression retains its original fixture.
- Existing snapshot-format-10 round-trip and collision checks passed. The fresh
  native fixture was used for the new eleven-branch Unity traversal checks.
- Release native library built and atomically installed in the working project.
  SHA256: `382e7fe75e43f584aead6c2ae3547b152cf3ce707c877c38d757981a3255d6b4`.

The live Unity scene was not restarted or visually inspected. Restart Unity to
load the native library. This is not a standalone player-build validation.
Existing emergency resource guards remain; extreme low ending/high branching
settings can still be expensive before descendant ending chances reach 1.

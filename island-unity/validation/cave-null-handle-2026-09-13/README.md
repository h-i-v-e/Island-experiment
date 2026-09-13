# Cave seam failure aborting island generation

The actual project log (Logs/Editor.log, lines 92787 and 92827 at investigation time) reported `CreateMotuWithCaveWalks: cave volume has an unmatched interior seam` immediately before Unity's null-island-handle exception. No failing island seed was present in the old diagnostic.

Reproduced that native error with a rotated/elevated cliff fixture, cave seed 11, the default walk/noise/voxel settings, and scene approach settings. A near-collapsed sliver left an edge with three triangle uses adjacent to a single-use edge. The seam pass only collected single-use edges, so the nearby unmatched endpoint never participated in the existing two-millimetre weld. Collecting odd-use edges repairs that sliver without increasing tolerance or disabling the closed-interior check. Five deterministic seed/orientation/height fixtures now cover this path. Each rotated fixture owns a copy of the source terrain so its coordinate changes stay isolated.

Native generation now retains a null-terminated UTF-8 failure reason per thread, including the island seed. Unity copies it immediately after a failed call, on the same worker, and includes it in the exception. C ABI options and mesh ownership are unchanged. Native/C# cave algorithm revisions are both 15, so cache keys incorporate the corrected meshing revision.

Validation:
- All 24 Rust cave tests passed, including the new seam regression and native error/thread-isolation test.
- The strengthened five-fixture seam test passed again after requiring every fixture to produce an entrance.
- Strict no-default-features library Clippy and cargo fmt passed.
- Release plugin rebuilt and atomically deployed with deploy-unity.sh.
- Built, deployed and isolated-test plugin SHA-256 all match: cea42ce915a7800513f6a68d4dfb419c5dcce86d05bca90f026425897e5f753f.

Restart the user's Unity editor to load the rebuilt native plugin. No live-editor visual acceptance or exact replay of the original failing island seed is claimed.

Unity 6000.5.6f1 / Metal isolated EditMode: 8 tests passed in 65.80 seconds. Covered ABI revision, native error propagation, normal generation/cancellation/installation/unload, empty LOD3 handling, sky/haze colours and the LOD3 rise. `git diff --check` passed.

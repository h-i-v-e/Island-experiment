# Terrain batch index remapping

The batching code assumed CombineMeshes concatenated complete source vertex buffers, and cached each tile's source indices plus the cumulative original vertex count. Unity omits unused source vertices when combining submeshes; those manually calculated indices can then refer past the combined buffer or to the wrong vertices.

Reproduced on Unity 6000.5.6f1 / Metal: the original code passed a two-triangle fixture without unused vertices but failed fixtures with 4 and 70,000 unused vertices per tile, logging the same out-of-bounds SetIndices error. These cover both 16-bit and 32-bit source/batch index formats.

The fix temporarily retains one combined submesh per tile and reads the actual combined indices for that tile, including Unity's base-vertex mapping. It then reduces the batch to one submesh. Tile visibility rebuilds still replace only the index list; vertices are combined once. The conservative sum of source vertex counts is used only to select index format.

The regression checks triangle positions and per-vertex material/environment data, initial combined rendering, hiding each tile, hiding the entire batch, restoring both tiles, and lazy debug edges. All three fixtures passed after the change, along with normal native generation/cancellation/installation/unload (4 tests total). Scoped git diff --check passed. Unrelated in-progress CoherentIslandFactory edits were preserved. No native plugin rebuild is required.

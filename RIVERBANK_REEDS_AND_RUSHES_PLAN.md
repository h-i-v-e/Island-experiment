# Riverbank Reeds and Rushes Plan

## Goal

Add deterministic, wind-animated reeds and rushes to final LOD 0 riverbanks
without decorating ordinary coastlines, creating one GameObject per clump, or
showing solid card rectangles in water reflections.

## Visual design

Each clump uses four double-sided card strips at 0, 45, 90, and 135 degrees.
Every strip has three vertical segments so its base remains pinned while the
upper stem silhouettes bend with the shared grass wind. A small set of
procedural alpha masks supplies tall reed and shorter rush variants, including
irregular stem widths, heights, leans, and occasional seed heads. Alpha clipping
and alpha-to-coverage provide stable depth and avoid transparent sorting.

## Placement contract

1. Use the final LOD 0 terrain, its exact river-bed mask, loose-cover depth,
   forced-rock mask, settled-stone vertices, and authoritative waterfall feet.
2. Seed the dry bank from non-river vertices adjacent to river-bed vertices and
   calculate a short graph distance outward across dry terrain.
3. Restrict roots to an above-sea, low-slope bank strip. Exclude forced rock,
   settled stones, waterfall impact clearances, and non-river coastlines.
4. Weight eligibility by loose-soil richness and deterministic coherent noise.
   Use the inner strip for taller reeds and the outer strip for shorter rushes.
5. Sample eligible triangles by physical area, then enforce deterministic local
   spacing with a spatial hash. Do not apply a silent island-wide count cap.
6. Feed the accepted root support vertices into forest placement as an explicit
   exclusion so tree trunks cannot overlap dense riverbank vegetation.

## Rust ownership and meshes

1. Add `island-rs/src/reeds.rs`; if it grows, use `reeds/placement.rs` and
   `reeds/mesh.rs` with no `mod.rs`.
2. Store one owned `ReedMeshes` value on `Island`. It owns deterministic clump
   metadata and complete LOD 0 owner-tile meshes.
3. Assign a whole clump to its 64x64 tile from its root position. Never slice
   card triangles at tile boundaries.
4. Encode card coordinates in UV0, clump root XZ in UV1, and variant, tint,
   stiffness, and phase data in vertex colour.
5. Export the 64x64 owner grid through `CreateReedMeshGrid`, reusing the existing
   `ExportMeshGrid` and `ReleaseMeshGrid` ownership contract.

## Unity streaming and rendering

1. Copy and validate the reed mesh grid on the generation worker.
2. Add a `ReedTileStreamer` owned by `TerrainTileStreamer`. It activates only
   the same 3x3 LOD 0 neighborhood as the terrain, producing at most nine reed
   renderers and no per-clump GameObjects.
3. Add a dedicated double-sided cutout shader that reuses
   `GrassWindCommon.cginc`. Wind displacement is quadratic by UV height and
   modified by per-clump stiffness and phase while the root remains fixed.
4. Use a coherent/stochastic distance fade before the active LOD 0 boundary to
   hide tile transitions.
5. Add a `MotuReflection="Reeds"` replacement subshader that retains the alpha
   mask and broad wind but uses simplified reflection lighting.
6. Expose visibility, bank width, patch scale, coverage, spacing, reed/rush
   ratio, height range, maximum slope, two colours, and wind strength through
   serializable settings with conservative defaults.

## Validation

- Rust: deterministic placement, river-only roots, sea/slope/rock/stone and
  waterfall exclusions, coherent clustering, spacing, whole-clump tile
  ownership, finite parallel mesh attributes, and FFI release behaviour.
- Unity: ABI sizes, copied attribute lengths, shader support, reflection tag,
  no per-clump objects, no active reeds outside LOD 0, bounded active renderer
  count, visibility toggling, regeneration cleanup, and scene/native batch
  validation.
- Live QA: inspect banks from all angles, estuaries, waterfall approaches,
  reflections, shadows, wind motion, and LOD transitions while watching frame
  time and draw calls.

## Delivery sequence

1. Placement and deterministic Rust tests.
2. Card mesh construction and owner tiles.
3. Native export and managed preparation.
4. LOD 0 Unity streaming and settings.
5. Main and simplified-reflection shaders.
6. Focused Rust, strict Clippy, native build/deploy, Unity batch validation, and
   live visual tuning.

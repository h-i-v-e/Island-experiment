# Dense radial forests

**Forests → Forest density** controls placement from 0.25–4, with a default of
2. The value is saved with the viewer settings; `--forest-density D` overrides
it. Old settings files automatically receive the new default. Dragging the
slider waits 350 ms after the last edit before rebuilding on the worker.

The equal-area spherical placement keeps the existing terrain/root queries,
water/channel clearance and slope exclusions. Seeded, bounded tangential jitter
breaks up the uniform spacing. A coherent coverage field retains forest patches
and clearings. The 64,000-tree safety limit uses deterministic reservoir sampling
rather than stopping at northern latitudes.

## Broadleaf and conifer biomes

Four existing Island broadleaf prototypes retain their detailed trunks and
foliage. Four lightweight conifer variants add irregular, overlapping evergreen
whorls, tapered crowns, visible trunks, height variation and a cooler green
palette. They share the existing radial wind, weather shadows and day/night
lighting. Conifers use geometry for their silhouette and do not need fur shells
or the broadleaf crown's column-shaped alpha holes.

Species selection uses a stable annual-temperature proxy based on the weather
model's latitude/elevation baseline: approximately `20 - 30*y² - 0.09*height`
Celsius, with small coherent regional variation. The probability blends between
broadleaf and conifer across 12–2 C. Trees thin and shrink between -4 and -10 C,
leaving a cold treeline rather than filling the ice caps. This is a placement
climate estimate, not a species switch driven by short-lived weather fronts.

**Inspect conifers**, **View → Conifers**, or `--view conifers` selects a lit,
colder stand. **Inspect forest** and `--view forest` remain available.

## Rendering cost

Planet already shares prototype geometry and materials between tree instances.
This implementation keeps that GPU instancing. Island's spatially merged crown
builder would require separate meshes for planetary patches and different LOD
ownership; it is not imported here. The inexpensive shared silhouettes and
bounded close detail are sufficient for the tested density increase.

- Distant broadleaf trees use three low-poly crown lobes and the existing Island
  LOD2 trunk in a single shared mesh: about 204 triangles per tree.
- Distant conifers use five whorls and a slender trunk: about 144 triangles.
- Close conifers have fewer than 400 foliage triangles and no fur shells.
- At most 96 closest trees receive detailed geometry; at most 12 broadleaf trees
  receive foliage shells. Remaining trees retain their shared distant crowns.
- Horizon cell culling, bounded uploads and asset reuse remain in place. Far
  closed crowns cull back faces; their visible silhouettes remain intact.
- Near-tree selection and membership work runs on movement/detail changes or
  pending uploads, rather than rebuilding a set every stationary frame.

## Validation

Release captures on seed 42, radius 2000 m, 1024 starting vertices and two
refinements produced 10,263 trees (3,003 conifers), compared with 4,423 before.
The same fixed orbital camera and graphics settings were used for the orbit
comparison. Separate forest inspection views were also checked visually; those
select a tree from each placement set and therefore do not have identical poses.
Clouds were disabled and solar time paused for both builds. Captures wait for
forest uploads and LOD preparation to settle. These are short stationary render
samples, not a guarantee for every terrain seed, density or GPU.

The automated viewer checks cover climate selection, conifer triangle bounds,
shared distant trunk geometry, radial roots, the near-detail/shell limits,
visibility handover, regeneration, CLI validation and preferences restoration.

Measured medians were approximately 15.35 ms before / 14.9 ms after in the first
fixed-orbit comparison, and 9.94 ms before / 9.89 ms after in the forest inspection
views. The conifer-dominated inspection measured 11.59 ms. These timings include
the complete rendered scene and are recorded under `work/forest-*.png.txt` in
the task workspace.

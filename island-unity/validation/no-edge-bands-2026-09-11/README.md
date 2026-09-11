# Remove decorative river and coastal bands — 2026-09-11

Removed the river-bank band and coastal overlay incoming/echo stripe paths,
including their unused material controls and runtime setup. Coastal tint and
patch fade remain. River surface detail, fast waterfall foam and refraction
remain; geometric ocean waves and their whitecaps are unchanged.

Unity 6000.5.6f1 / Metal, isolated project: **8 passed, 0 failed** in
RiverSurfaceTests and OceanOpticsTests. The rendered fixture now also compiles
and renders the coastal tint-only material. Inspected both included images:
the river edge has no decorative band and the coastal overlay has no stripes.
These are controlled fixtures, not generated-island acceptance tests.
`git diff --check` passed.

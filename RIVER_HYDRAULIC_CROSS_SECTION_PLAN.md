# Progressive River Ring Plan

## Goal

Form each river as a simple widening mesh corridor after its waterfall profile
has been established. Width is shaped first; depth is calculated from the
resulting width second; the whole selected corridor is then carved and
smoothed.

This replaces the previous area-capacity solver, local tessellation, rim-only
pulling, and flood-filled bank grading.

## Implemented sequence

1. Trace the river path and establish its water surfaces.
2. Add and relocate waterfalls before changing channel width.
3. Begin at the configured source width, then derive monotonically increasing
   width and nominal-depth targets from per-river flow and path progress.
4. Convert the desired half-width into a discrete terrain-ring count.
5. Move every selected ring from the centre outward to its proportional target
   position: narrow upstream corridors are compressed inward and wider
   downstream corridors are expanded outward.
6. Add another terrain ring whenever the target width crosses the next local
   mesh-spacing threshold.
7. Remeasure the represented width from the shaped banks.
8. Calculate depth as nominal depth plus a bounded linear width correction:
   narrow sections deepen and broad sections become shallower.
9. Carve every selected corridor ring to the same local floor as its owning
   centreline node.
10. Lower three additional land rings outside the corridor with a smooth
    falloff to their original heights. The removed material feeds the owning
    river's sediment budget and therefore the existing delta deposition pass.
11. Smooth the complete corridor, including the outer bank ring. Bank vertices
    include surrounding terrain in their average so the floor blends back into
    the landscape.
12. Preserve the existing waterfall, confluence, submerged-mouth, sea clipping,
    material, river mesh, and UV passes.

## Deliberate limits

- Width correction is linear, not reciprocal or area-based.
- Depth compensation is limited to 50 percent above or below nominal depth.
- The configured maximum depth remains an absolute upper bound.
- Horizontal ring movement is limited by triangle-orientation safety checks.
- Channel-ring selection changes discretely, but target width within a ring
  grows continuously downstream.
- Default full widths are 2 metres at the source and 14 metres at the terminal
  end; both remain editable in Unity.
- Smoothing may raise a vertex only when the river pass actually carved it,
  and never above its original ground height or the water-clearance ceiling.
  Naturally low terrain is never raised to the calculated floor.

## Removed behavior

- no cross-sectional area conservation solver;
- no inverse-width depth calculation;
- no propagation of one deep sample across an entire waterfall reach;
- no local tessellation around pulled rims;
- no inward-only rim correction;
- no unbounded bank flood-fill; the replacement valley apron is exactly three
  land rings and has a smooth height falloff;
- no waterfall-shelf bank deposition special case.

## Acceptance criteria

- channel target width and nominal depth never decrease downstream;
- the ring count expands as the requested width crosses mesh-spacing
  thresholds;
- ring shaping can move banks both inward and outward without changing mesh
  topology or inverting projected triangles;
- exact-width sections use nominal depth;
- narrower sections are deeper and wider sections are shallower, with both
  corrections bounded to 50 percent;
- every selected ring is initially carved to its owning centreline floor;
- smoothing includes bank vertices and keeps all corridor vertices between the
  applicable lower limit and the lesser of original ground height and the
  water-clearance ceiling;
- three surrounding land rings descend smoothly toward the river, while ocean
  vertices remain available for delta deposition;
- valley erosion contributes sediment to the existing delta budget;
- waterfall profiles are established before ring shaping;
- river meshes remain hard-clipped at sea level;
- focused river tests, a short complete-island probe, strict Clippy, formatting,
  and release-library construction pass.

## Current verification

- focused river suite: 40 tests pass;
- short complete-island generation probe passes;
- generated probe mean represented width: 9.497 metres;
- generated probe maximum represented width: 24.080 metres;
- generated probe mean channel depth: 1.038 metres;
- generated probe maximum channel depth: 2.000 metres;
- long-running generation suites remain intentionally skipped.

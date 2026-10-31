//! One frozen width field for the river banks, refinement and waterfall profile.
//! Like Island, blend flow growth with progress along the represented network.
use super::{Drainage, RiverSegment, TerrainOptions, graph::arc};
use crate::PlanetMesh;

pub(super) fn legacy(catchment: f64, settings: &TerrainOptions) -> f64 {
    let threshold = settings.river_catchment * 4.0 * std::f64::consts::PI;
    settings.river_width * (catchment / threshold).sqrt().clamp(1.0, 3.0)
}

pub(super) fn build(
    mesh: &PlanetMesh,
    drainage: &Drainage,
    rivers: &[RiverSegment],
    settings: &TerrainOptions,
) -> Vec<f64> {
    let mut widths: Vec<_> = drainage
        .catchment
        .iter()
        .map(|&q| legacy(q, settings))
        .collect();
    if settings.river_taper == 0.0 || rivers.is_empty() {
        return widths;
    }
    let count = mesh.directions.len();
    let mut active = vec![false; count];
    for river in rivers {
        active[river.from] = true;
        active[river.to] = true;
    }
    let mut outlet_flow = vec![0.0_f64; count];
    let mut downstream_length = vec![0.0; count];
    let mut upstream_length = vec![0.0_f64; count];
    // The priority flood supplies parent-before-child order; no sorting or
    // recursive walks per river/vertex are needed, even at complex confluences.
    for &i in &drainage.order {
        if !active[i] {
            continue;
        }
        if let Some(to) = drainage.receivers[i].filter(|&to| active[to]) {
            outlet_flow[i] = outlet_flow[to];
            downstream_length[i] =
                downstream_length[to] + arc(mesh.directions[i], mesh.directions[to]);
        } else {
            outlet_flow[i] = drainage.catchment[i];
        }
    }
    for &i in drainage.order.iter().rev() {
        if active[i]
            && let Some(to) = drainage.receivers[i].filter(|&to| active[to])
        {
            upstream_length[to] = upstream_length[to]
                .max(upstream_length[i] + arc(mesh.directions[i], mesh.directions[to]));
        }
    }
    let source_flow = (settings.river_catchment * 4.0 * std::f64::consts::PI).sqrt();
    for (i, width) in widths.iter_mut().enumerate().filter(|(i, _)| active[*i]) {
        let length = upstream_length[i] + downstream_length[i];
        let path_growth = if length > 0.0 {
            upstream_length[i] / length
        } else {
            1.0
        };
        let flow_span = outlet_flow[i].sqrt() - source_flow;
        let flow_growth = if flow_span > 1e-12 {
            ((drainage.catchment[i].sqrt() - source_flow) / flow_span).clamp(0.0, 1.0)
        } else {
            path_growth
        };
        let growth = f64::midpoint(flow_growth, path_growth);
        let mouth = legacy(outlet_flow[i], settings);
        // Narrow sources but retain the existing mouth width. A minimum matches
        // the adaptive 0.2m edge spacing on a 1km planet. Never widen old reaches.
        let source = (mouth * 0.15).max(0.0001).min(mouth);
        let target = (source + (mouth - source) * growth).min(*width);
        *width += (target - *width) * settings.river_taper;
    }
    widths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DVec3;
    fn network() -> (PlanetMesh, Drainage, Vec<RiverSegment>, TerrainOptions) {
        let settings = TerrainOptions::default();
        let q = settings.river_catchment * 4.0 * std::f64::consts::PI;
        // 0--1--2--3--4(sea), with 5--6--2 entering as a tributary.
        let mesh = PlanetMesh {
            directions: [
                (0., 0.),
                (0.01, 0.),
                (0.02, 0.),
                (0.03, 0.),
                (0.04, 0.),
                (0.01, 0.02),
                (0.02, 0.01),
            ]
            .map(|(x, y)| DVec3::new(x, y, 1.).normalize())
            .to_vec(),
            triangles: Vec::new(),
            radius: 1000.,
        };
        let drainage = Drainage {
            receivers: vec![Some(1), Some(2), Some(3), Some(4), None, Some(6), Some(2)],
            catchment: [1., 3., 13., 24., 25., 1., 4.].map(|v| v * q).to_vec(),
            order: vec![4, 3, 2, 1, 0, 6, 5],
        };
        let rivers = drainage
            .receivers
            .iter()
            .enumerate()
            .filter_map(|(from, &to)| {
                Some(RiverSegment {
                    from,
                    to: to?,
                    catchment: drainage.catchment[from],
                })
            })
            .collect();
        (mesh, drainage, rivers, settings)
    }
    #[test]
    fn width_grows_downstream_through_confluences_and_beyond_the_old_flow_cap() {
        let (mesh, drainage, rivers, settings) = network();
        let widths = build(&mesh, &drainage, &rivers, &settings);
        for r in &rivers {
            assert!(widths[r.from] <= widths[r.to]);
        }
        assert!(widths[0] < widths[4] * 0.2);
        assert!(widths[5] < widths[2]);
        assert!(
            widths[2] < widths[3],
            "upstream channels must keep narrowing even after the old width cap"
        );
        assert_eq!(
            widths[4].to_bits(),
            legacy(drainage.catchment[4], &settings).to_bits()
        );
        for (i, &w) in widths.iter().enumerate() {
            assert!(w <= legacy(drainage.catchment[i], &settings));
            assert!(w >= 0.0001);
        }
    }
    #[test]
    fn zero_taper_is_exactly_legacy_and_constant_flow_still_tapers_by_distance() {
        let (mesh, mut drainage, mut rivers, mut settings) = network();
        settings.river_taper = 0.0;
        assert_eq!(
            build(&mesh, &drainage, &rivers, &settings),
            drainage
                .catchment
                .iter()
                .map(|&q| legacy(q, &settings))
                .collect::<Vec<_>>()
        );
        let q = drainage.catchment[0];
        drainage.catchment.fill(q);
        for r in &mut rivers {
            r.catchment = q;
        }
        settings.river_taper = 1.0;
        let widths = build(&mesh, &drainage, &rivers, &settings);
        for r in &rivers {
            assert!(widths[r.from] < widths[r.to]);
        }
    }
}

mod frontage;
mod network;

pub(super) use frontage::{
    Access, Lot, Lots, PAVED_TREAD_OVERLAP, Row, approach, central_approach, entrance, lots,
    stair_access, stair_target,
};
pub(super) use network::{Streets, layout};

use super::{ClearanceRect, TownInput, TownRoadExit, TownSettings, placement};
use motu::{
    Vec2,
    roads::{RoadNetwork, RoadStrip},
};

fn corridor(from: Vec2, to: Vec2, half_width: f32, end_margin: f32) -> ClearanceRect {
    let delta = to - from;
    ClearanceRect {
        centre: ((from + to) * 0.5).to_array(),
        half_extents: [delta.length() * 0.5 + end_margin, half_width],
        yaw: delta.y.atan2(delta.x),
    }
}

fn strip_ends(strip: &RoadStrip) -> (Vec2, Vec2) {
    (
        strip.stations[0].point,
        strip.stations[strip.stations.len() - 1].point,
    )
}

/// Select at most one unobstructed departure per outside face. Prefer the
/// middle of a side, where there is room for a cart to turn off the street.
pub(super) fn clear_curb_exits(
    mut candidates: Vec<TownRoadExit>,
    centre: [f32; 2],
    roads: &RoadNetwork,
    buildings: &[ClearanceRect],
    reservations: &[ClearanceRect],
    input: &TownInput,
    settings: &TownSettings,
) -> Vec<TownRoadExit> {
    let xy = |exit: &TownRoadExit| Vec2::new(exit.position[0], exit.position[1]);
    let offset = |exit: &TownRoadExit| {
        (xy(exit) - Vec2::from_array(centre))
            .perp_dot(Vec2::from_array(exit.direction))
            .abs()
    };
    candidates.sort_by(|a, b| offset(a).total_cmp(&offset(b)));
    let mut selected = Vec::<TownRoadExit>::new();
    for exit in candidates {
        let direction = Vec2::from_array(exit.direction);
        if selected
            .iter()
            .any(|other| direction.dot(Vec2::from_array(other.direction)) > 0.99)
        {
            continue;
        }
        let curb = xy(&exit);
        let end = curb + direction * 4.;
        // Start just beyond the host strip so a touching curb does not count as crossing it.
        let departure = corridor(curb + direction * 0.01, end, 0.9, 0.);
        if obstructs(roads, &departure, false)
            || placement::overlaps_any(&departure, buildings)
            || placement::overlaps_any(&departure, reservations)
            || placement::protected(&departure, input)
            || placement::touches_river(&departure, input, settings.river_margin)
        {
            continue;
        }
        if [0., 2., 4.].into_iter().all(|along| {
            [-0.9, 0., 0.9].into_iter().all(|across| {
                let point = curb + direction * along + direction.perp() * across;
                input
                    .terrain
                    .sample(point.x, point.y)
                    .is_some_and(|h| h > input.sea_level + settings.min_dry_height)
            })
        }) {
            selected.push(exit);
        }
    }
    selected
}

fn corridors(
    roads: &RoadNetwork,
    margin: f32,
    include_shoulder: bool,
) -> impl Iterator<Item = ClearanceRect> + '_ {
    let shoulder = if include_shoulder { roads.shoulder } else { 0. };
    roads
        .strips
        .iter()
        .map(move |strip| {
            let (a, b) = strip_ends(strip);
            corridor(
                a,
                b,
                strip.half_width + shoulder + margin,
                shoulder + margin,
            )
        })
        .chain(roads.junctions.iter().filter_map(move |junction| {
            let (axis, half) = junction
                .rectangle()
                .unwrap_or((Vec2::X, Vec2::splat(junction.radius)));
            (junction.radius > 0.).then_some(ClearanceRect {
                centre: junction.point.to_array(),
                half_extents: (half + Vec2::splat(shoulder + margin)).to_array(),
                yaw: axis.to_angle(),
            })
        }))
}

pub(super) fn obstructs(roads: &RoadNetwork, rect: &ClearanceRect, shoulder: bool) -> bool {
    let shoulder = if shoulder { roads.shoulder } else { 0. };
    roads.strips.iter().any(|strip| {
        let (a, b) = strip_ends(strip);
        let road = corridor(a, b, strip.half_width + shoulder, shoulder);
        super::placement::overlaps(rect, &road, 0.)
    }) || roads.junctions.iter().any(|junction| {
        if let Some((axis, half)) = junction.rectangle() {
            let pavement = ClearanceRect {
                centre: junction.point.to_array(),
                half_extents: (half + Vec2::splat(shoulder)).to_array(),
                yaw: axis.to_angle(),
            };
            return super::placement::overlaps(rect, &pavement, 0.);
        }
        let local = Vec2::from_array(rect.local(junction.point.to_array()));
        let distance = (local.abs() - Vec2::from_array(rect.half_extents)).max(Vec2::ZERO);
        distance.length_squared() < (junction.radius + shoulder).powi(2)
    })
}

pub(super) fn clearance(
    roads: &RoadNetwork,
    margin: f32,
) -> impl Iterator<Item = ClearanceRect> + '_ {
    corridors(roads, margin, true)
}

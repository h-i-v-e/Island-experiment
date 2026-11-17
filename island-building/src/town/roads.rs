mod frontage;
mod network;

pub(super) use frontage::{Access, Lot, approach, central_approach, entrance, lot};
pub(super) use network::{Streets, layout};

use super::ClearanceRect;
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
            let radius = junction.radius + shoulder + margin;
            (junction.radius > 0.).then_some(ClearanceRect {
                centre: junction.point.to_array(),
                half_extents: [radius; 2],
                yaw: 0.,
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

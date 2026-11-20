#![allow(clippy::float_cmp)] // These dimensions and positions are copied unchanged.

use super::*;

const CAPITAL: Vec2 = Vec2::new(0., 65.);
const BOUNDS: [Vec2; 2] = [Vec2::new(-120., -170.), Vec2::new(120., 100.)];
const SEA: f32 = 2.;

fn coast(point: Vec2) -> f32 {
    SEA + if point.y < 0. {
        (point.y * 0.42).max(-15.)
    } else {
        (point.y * 0.14).min(4.)
    }
}

fn fixture(
    terrain: &impl Fn(Vec2) -> Option<f32>,
    shore_allowed: &impl Fn(Vec2) -> bool,
) -> Option<PierLayout> {
    plan_pier(CAPITAL, SEA, BOUNDS, terrain, shore_allowed)
}

#[test]
fn nearest_landing_gets_the_shortest_valid_berth_and_a_straight_arrival() {
    let terrain = |point| Some(coast(point));
    let layout = fixture(&terrain, &|_| true).expect("a gentle beach with deep water fits");
    assert_eq!(Some(layout), fixture(&terrain, &|_| true));
    assert!(layout.shore.x.abs() < 0.01);
    assert!((-0.03..0.).contains(&layout.shore.y));
    assert_eq!(layout.berth_side, 1.);
    assert_eq!(layout.axis, Vec2::NEG_Y);
    assert_eq!(layout.berth.heading, -layout.axis);
    assert_eq!(layout.berth.position.z, SEA);
    assert_eq!(layout.berth.approach.z, SEA);
    assert!((layout.end.distance(layout.shore) - 54.).abs() < 0.01);
    assert!(layout.berth.min_depth >= 5.);
    assert_eq!(layout.berth.length, 42.);
    assert_eq!(layout.berth.width, 16.);
    let arrival = layout.berth.position - layout.berth.approach;
    assert!(arrival.truncate().normalize().dot(layout.berth.heading) > 0.99);
    assert!(arrival.length() > layout.berth.length);
    assert_eq!(layout.start.z, coast(layout.start.truncate()));
}

#[test]
fn blocked_waterfront_uses_nearest_other_landing_instead() {
    let terrain = |point| Some(coast(point));
    let layout = fixture(&terrain, &|point| point.x.abs() >= 17.5)
        .expect("either side has a usable landing");
    assert!((layout.shore.x.abs() - 18.).abs() < 0.01);
    assert!(layout.start.x.abs() >= 17.5);
}

#[test]
fn rejects_isolated_deep_water_behind_a_reef() {
    let terrain = |point: Vec2| {
        Some(if (-149. ..=-145.).contains(&point.y) {
            SEA - 1.
        } else {
            coast(point)
        })
    };
    assert_eq!(None, fixture(&terrain, &|point| point.x.abs() < 0.5));
}

#[test]
fn rejects_a_narrow_channel_with_a_deep_centerline() {
    let terrain = |point: Vec2| {
        Some(if point.y < 0. && point.x.abs() > 5. {
            coast(point).max(SEA - 2.)
        } else {
            coast(point)
        })
    };
    assert_eq!(None, fixture(&terrain, &|_| true));
}

#[test]
fn missing_depth_inside_the_berth_rejects_the_shorter_side() {
    let terrain = |point: Vec2| {
        if (6. ..16.).contains(&point.x) && (-62. ..=-18.).contains(&point.y) {
            None
        } else {
            Some(coast(point))
        }
    };
    let layout = fixture(&terrain, &|point| point.x.abs() < 0.5)
        .expect("the opposite side has a usable berth");
    assert_eq!(layout.berth_side, -1.);
}

#[test]
fn a_steep_bank_cannot_hide_beneath_the_quay() {
    let terrain = |point: Vec2| Some(if point.y > 0. { SEA + 6. } else { coast(point) });
    assert_eq!(None, fixture(&terrain, &|_| true));
}

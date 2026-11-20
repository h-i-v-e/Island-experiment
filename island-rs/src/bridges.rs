//! Terrain-fitted masonry bridges in island-local XY metres with Z-up heights.

mod mesh;

pub use mesh::{BridgeMeshes, build_bridge_meshes};

use crate::{Vec2, Vec3};
use serde::{Deserialize, Deserializer, Serialize};

pub(crate) const MIN_HEADING_DOT: f32 = 0.8;
const MAX_GRADE: f32 = 0.12;
// Maximum derivative of the unit crest 16 t²(1-t)² is 16/(3 sqrt(3)).
const CREST_PEAK_SLOPE: f32 = 3.079_202;
const COVER: f32 = 0.50;
const PIER: f32 = 1.4;
const FOOTING_EMBED: f32 = 0.45;
const WALL: f32 = 0.42;
const MIN_OPENING: f32 = 0.30;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeKind {
    River,
    Ravine,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeSurface {
    Paved,
    CartTrack,
}

/// Preferred clear span between bearing faces for the ordinary relief arches.
/// The primary channel can require a wider arch, and short bank shelves a smaller one.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct BridgeSpanProfile {
    pub min_clear_span: f32,
    pub max_clear_span: f32,
}

impl Default for BridgeSpanProfile {
    fn default() -> Self {
        Self {
            min_clear_span: 6.,
            max_clear_span: 10.,
        }
    }
}

impl BridgeSpanProfile {
    fn validate(self) -> Result<(), String> {
        if (1. ..=64.).contains(&self.min_clear_span)
            && (self.min_clear_span..=64.).contains(&self.max_clear_span)
        {
            Ok(())
        } else {
            Err("bridge profile needs ordered finite clear spans between 1 and 64 metres".into())
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Rounded positive count within the 1024 m deck limit.
    fn bay_count(self, length: f32) -> usize {
        let target = (self.min_clear_span + self.max_clear_span) * 0.5;
        let nominal = ((length + PIER) / (target + PIER)).round();
        nominal
            .max(((length + PIER) / (self.max_clear_span + PIER)).ceil())
            .max(1.) as usize
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BridgeChannel {
    pub center: [f32; 2],
    /// Local direction of downstream flow.
    pub direction: [f32; 2],
    /// Half-width of the main channel that must be free of piers.
    pub half_width: f32,
    pub water_height: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BridgeRequest {
    pub start: [f32; 3],
    pub end: [f32; 3],
    /// Approach headings in the direction of travel from start to end.
    /// They check the bank turns. The bridge follows the endpoint chord.
    #[serde(default, deserialize_with = "optional_heading")]
    pub start_heading: Option<[f32; 2]>,
    #[serde(default, deserialize_with = "optional_heading")]
    pub end_heading: Option<[f32; 2]>,
    /// Clear carriageway width between the parapets.
    pub width: f32,
    pub kind: BridgeKind,
    pub surface: BridgeSurface,
    #[serde(default)]
    pub channel: Option<BridgeChannel>,
}

fn optional_heading<'de, D: Deserializer<'de>>(reader: D) -> Result<Option<[f32; 2]>, D::Error> {
    // Unity can serialize an unassigned optional array as [].
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Heading {
        Vector([f32; 2]),
        Empty([f32; 0]),
    }
    Ok(
        Option::<Heading>::deserialize(reader)?.and_then(|heading| match heading {
            Heading::Vector(v) => Some(v),
            Heading::Empty(_) => None,
        }),
    )
}

#[derive(Clone, Copy, Debug)]
pub struct BridgeBay {
    pub start: f32,
    pub end: f32,
    pub spring: f32,
    pub rise: f32,
}

impl BridgeBay {
    /// Height of the underside of the circular segment at a bridge station.
    #[must_use]
    pub fn intrados(&self, station: f32) -> f32 {
        let half = (self.end - self.start) * 0.5;
        let x = (station - self.start - half).clamp(-half, half);
        let radius = (half * half + self.rise * self.rise) / (2. * self.rise);
        self.spring + self.rise - x * x / (radius + (radius * radius - x * x).sqrt())
    }

    fn middle(self) -> f32 {
        (self.start + self.end) * 0.5
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BridgeSupport {
    pub start: f32,
    pub end: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug)]
pub struct BridgeLayout {
    pub start: Vec3,
    pub end: Vec3,
    pub axis: Vec2,
    /// Stations measure metres along the straight bridge axis.
    pub length: f32,
    pub width: f32,
    pub kind: BridgeKind,
    pub surface: BridgeSurface,
    pub bays: Vec<BridgeBay>,
    pub supports: Vec<BridgeSupport>,
    pub channel: Option<BridgeChannel>,
    crest: f32,
}

impl BridgeLayout {
    fn from_request(request: &BridgeRequest) -> Result<Self, String> {
        let start = Vec3::from_array(request.start);
        let end = Vec3::from_array(request.end);
        let chord = (end - start).truncate();
        let length = chord.length();
        if !start.is_finite()
            || !end.is_finite()
            || !(4. ..=1024.).contains(&length)
            || !(0.8..=24.).contains(&request.width)
        {
            return Err("invalid bridge endpoints, length or carriageway width".into());
        }
        if (end.z - start.z).abs() > length * MAX_GRADE {
            return Err("bridge endpoints exceed the maximum road grade".into());
        }
        let axis = chord / length;
        for heading in [request.start_heading, request.end_heading] {
            let dir = heading.map_or(axis, Vec2::from_array).normalize_or_zero();
            if !dir.is_finite() || axis.dot(dir) < MIN_HEADING_DOT {
                return Err("approach heading turns too sharply onto the bridge".into());
            }
        }
        Ok(Self {
            start,
            end,
            axis,
            length,
            width: request.width,
            kind: request.kind,
            surface: request.surface,
            channel: request.channel,
            crest: 0.,
            bays: Vec::new(),
            supports: Vec::new(),
        })
    }

    /// The horizontal position at a station, with positive offsets to the left.
    #[must_use]
    pub fn point(&self, station: f32, lateral: f32) -> Vec2 {
        let t = (station / self.length).clamp(0., 1.);
        self.start.truncate().lerp(self.end.truncate(), t) + self.axis.perp() * lateral
    }

    #[must_use]
    pub fn deck_height(&self, station: f32) -> f32 {
        let t = (station / self.length).clamp(0., 1.);
        self.start.lerp(self.end, t).z + self.crest * crest_weight(t)
    }

    // Raise a smooth crest only as far as the fixed endpoint grades permit.
    fn require_height(&mut self, station: f32, height: f32) -> Result<(), String> {
        let t = station / self.length;
        let deficit = height - self.start.lerp(self.end, t).z;
        if deficit <= 0. {
            return Ok(());
        }
        let crown = deficit / crest_weight(t);
        let grade = ((self.end.z - self.start.z).abs() + CREST_PEAK_SLOPE * crown) / self.length;
        if !grade.is_finite() || grade > MAX_GRADE {
            return Err(
                "terrain or water leaves insufficient bridge clearance at the approach grade"
                    .into(),
            );
        }
        self.crest = self.crest.max(crown);
        Ok(())
    }
}

fn crest_weight(t: f32) -> f32 {
    (4. * t * (1. - t)).powi(2)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)] // At most 1024 m at fixed positive spacing.
fn stations(start: f32, end: f32, spacing: f32) -> impl Iterator<Item = f32> {
    let intervals = ((end - start) / spacing).ceil().max(1.) as u32;
    (0..=intervals).map(move |i| start + (end - start) * (i as f32 / intervals as f32))
}

fn terrain(sample: &impl Fn(Vec2) -> Option<f32>, p: Vec2) -> Result<f32, String> {
    sample(p)
        .filter(|z| z.is_finite())
        .ok_or_else(|| "missing terrain under bridge footprint".into())
}

#[derive(Clone, Copy)]
struct ChannelInterval {
    start: f32,
    end: f32,
    center: f32,
}

/// Intersect the river centerline with a line along the bridge at this lateral offset.
fn flow_intersection(
    layout: &BridgeLayout,
    channel: BridgeChannel,
    lateral: f32,
) -> Result<f32, String> {
    let flow = Vec2::from_array(channel.direction);
    let origin = Vec2::from_array(channel.center);
    let offset = flow.perp_dot(layout.point(0., lateral) - origin);
    let station = -offset / flow.perp_dot(layout.axis);
    if !(0. ..=layout.length).contains(&station) {
        return Err("bridge approach falls inside or alongside the river".into());
    }
    Ok(station)
}

fn protected_channel(layout: &BridgeLayout) -> Result<Option<ChannelInterval>, String> {
    let Some(channel) = layout.channel else {
        return Ok(None);
    };
    let flow = Vec2::from_array(channel.direction).normalize_or_zero();
    let origin = Vec2::from_array(channel.center);
    if !origin.is_finite()
        || !flow.is_finite()
        || flow == Vec2::ZERO
        || !channel.half_width.is_finite()
        || channel.half_width <= 0.
        || !channel.water_height.is_finite()
    {
        return Err("invalid protected bridge channel".into());
    }
    let offset = |point| flow.perp_dot(point - origin);
    if offset(layout.start.truncate()) * offset(layout.end.truncate()) >= 0. {
        return Err("bridge endpoints must be on opposite banks".into());
    }
    let mut start = f32::INFINITY;
    let mut end = f32::NEG_INFINITY;
    let center = flow_intersection(layout, channel, 0.)?;
    // Cutwater tips extend along the flow, leaving their cross-channel
    // projection unchanged. Include both outside faces of every support.
    let reach = flow.perp_dot(layout.axis.perp()).abs() * (layout.width * 0.5 + WALL);
    for s in stations(0., layout.length, 0.25) {
        let dist = offset(layout.point(s, 0.)).abs();
        if dist <= channel.half_width + reach + 0.35 {
            start = start.min(s - 0.25);
            end = end.max(s + 0.25);
        }
    }
    Ok(Some(ChannelInterval { start, end, center }))
}

#[allow(clippy::cast_precision_loss)] // The bounded span and pier width keep bay counts exactly representable.
fn regular_bays(bays: &mut Vec<BridgeBay>, start: f32, end: f32, profile: BridgeSpanProfile) {
    let count = profile.bay_count(end - start);
    let pitch = (end - start + PIER) / count as f32;
    for i in 0..count {
        let a = start + i as f32 * pitch;
        let b = if i + 1 == count {
            end
        } else {
            a + pitch - PIER
        };
        bays.push(new_bay(a, b));
    }
}

fn new_bay(start: f32, end: f32) -> BridgeBay {
    BridgeBay {
        start,
        end,
        spring: 0.,
        rise: (end - start) * 0.17,
    }
}

fn arrange_bays(
    layout: &mut BridgeLayout,
    channel: Option<ChannelInterval>,
    profile: BridgeSpanProfile,
) -> Result<(), String> {
    let seat = (layout.length * 0.075).clamp(0.65, 2.);
    let last = layout.length - seat;
    let Some(channel) = channel else {
        regular_bays(&mut layout.bays, seat, last, profile);
        return Ok(());
    };
    if channel.start < seat || channel.end > last {
        return Err("protected channel does not leave room for both abutments".into());
    }
    let ordinary_min = profile.min_clear_span + PIER;
    let shelf = (channel.center - seat).min(last - channel.center);
    // Keep both relief arches near their preferred span where the available
    // bank shelves permit. The protected interval always wins.
    let main_target = (profile.max_clear_span * 1.05)
        .min(2. * (shelf - ordinary_min))
        .max(profile.min_clear_span * 1.15);
    let main_width = (channel.end - channel.start)
        .max(main_target)
        .min(last - seat);
    let short_shelf = PIER + profile.min_clear_span * 0.65;
    let mut a = (channel.center - main_width * 0.5)
        .max(channel.end - main_width)
        .min(channel.start)
        .max(seat)
        .min((last - main_width).max(seat));
    let mut b = (a + main_width).min(last);
    if a - seat >= short_shelf {
        regular_bays(&mut layout.bays, seat, a - PIER, profile);
    } else {
        a = seat;
    }
    let has_right = last - b >= short_shelf;
    if !has_right {
        b = last;
    }
    layout.bays.push(new_bay(a, b));
    if has_right {
        regular_bays(&mut layout.bays, b + PIER, last, profile);
    }
    Ok(())
}

fn fit_deck(
    layout: &mut BridgeLayout,
    sample: &impl Fn(Vec2) -> Option<f32>,
) -> Result<(), String> {
    // Bank crossfall and small road cuts can cover the first few decimetres
    // of the shoulder. Beyond the landing, keep the carriageway exposed.
    for s in stations(0., layout.length, 0.75) {
        let to_bank = s.min(layout.length - s);
        let tolerance = 0.04 + 0.31 * (1. - to_bank / 2.).max(0.);
        for lateral in [-layout.width * 0.5, 0., layout.width * 0.5] {
            let ground = terrain(sample, layout.point(s, lateral))?;
            layout.require_height(s, ground - tolerance)?;
        }
    }
    Ok(())
}

// A horizontal spring line must leave masonry cover below a graded, possibly
// crowned roadway. Bound each interval using its highest intrados departure
// and lowest deck/crest weight (the crest has only an interior maximum).
fn arch_intervals(bay: BridgeBay) -> impl Iterator<Item = (f32, f32, f32)> {
    let mut a = bay.start;
    stations(bay.start, bay.end, 0.5).skip(1).map(move |b| {
        let top = bay.intrados(bay.middle().clamp(a, b)) - bay.spring;
        let interval = (a, b, top);
        a = b;
        interval
    })
}

fn fit_openings(
    layout: &mut BridgeLayout,
    channel: Option<ChannelInterval>,
    sample: &impl Fn(Vec2) -> Option<f32>,
) -> Result<(), String> {
    // The main opening is the flow centerline or the bay above the deepest
    // measured depression. Side arches may spring into rising valley banks.
    let available_crest =
        (MAX_GRADE * layout.length - (layout.end.z - layout.start.z).abs()) / CREST_PEAK_SLOPE;
    let mut deepest = f32::NEG_INFINITY;
    let mut primary = None;
    for bay in &layout.bays {
        if channel.is_some_and(|c| c.center < bay.start || c.center > bay.end) {
            continue;
        }
        let s = channel.map_or(bay.middle(), |c| c.center);
        let bed = terrain(sample, layout.point(s, 0.))?;
        let floor = layout.channel.map_or(bed, |c| bed.max(c.water_height));
        let depth = layout.start.lerp(layout.end, s / layout.length).z
            + available_crest * crest_weight(s / layout.length)
            - floor;
        if depth > deepest {
            deepest = depth;
            primary = Some((*bay, floor + MIN_OPENING - bay.intrados(s)));
        }
    }
    let (main, mut min_spring) = primary.ok_or("no open arch across the main channel")?;
    if let Some(channel) = layout.channel {
        let half = layout.width * 0.5 + WALL;
        for lateral in stations(-half, half, 0.75) {
            let s = flow_intersection(layout, channel, lateral)?;
            let ground = terrain(sample, layout.point(s, lateral))?;
            let floor = ground.max(channel.water_height);
            min_spring = min_spring.max(floor + MIN_OPENING - main.intrados(s));
        }
    }
    for (a, b, rise) in arch_intervals(main) {
        // Check the lower road endpoint against the smaller crest weight.
        let required = min_spring + rise + COVER;
        let base = |s| layout.start.lerp(layout.end, s / layout.length).z;
        let min_base = base(a).min(base(b));
        let critical = if crest_weight(a / layout.length) < crest_weight(b / layout.length) {
            a
        } else {
            b
        };
        layout.require_height(critical, base(critical) + required - min_base)?;
    }
    for i in 0..layout.bays.len() {
        let bay = layout.bays[i];
        layout.bays[i].spring = arch_intervals(bay)
            .map(|(a, b, rise)| {
                let t0 = a / layout.length;
                let t1 = b / layout.length;
                let base = layout
                    .start
                    .lerp(layout.end, t0)
                    .z
                    .min(layout.start.lerp(layout.end, t1).z);
                base + layout.crest * crest_weight(t0).min(crest_weight(t1)) - COVER - rise
            })
            .fold(f32::INFINITY, f32::min);
    }
    Ok(())
}

fn fit_supports(
    layout: &mut BridgeLayout,
    sample: &impl Fn(Vec2) -> Option<f32>,
) -> Result<(), String> {
    let flow = layout
        .channel
        .map(|c| Vec2::from_array(c.direction).normalize());
    for i in 0..=layout.bays.len() {
        let left = i.checked_sub(1).map(|n| layout.bays[n]);
        let right = layout.bays.get(i).copied();
        let a = left.map_or(0., |bay| bay.end);
        let b = right.map_or(layout.length, |bay| bay.start);
        let half = layout.width * 0.5 + WALL;
        let mut floor = f32::INFINITY;
        for s in stations(a, b, 0.5) {
            for y in stations(-half, half, 0.85) {
                floor = floor.min(terrain(sample, layout.point(s, y))?);
            }
        }
        if let (Some(flow), Some(_), Some(_)) = (flow, left, right) {
            let mid = (a + b) * 0.5;
            let normal = layout.axis.perp();
            let outward = flow * flow.dot(normal).signum();
            let projection = (0.85 * (b - a)).min(1.4);
            for sign in [-1., 1.] {
                let tip = layout.point(mid, sign * half) + sign * outward * projection;
                floor = floor.min(terrain(sample, tip)?);
            }
        }
        let spring = left
            .into_iter()
            .chain(right)
            .map(|bay| bay.spring)
            .fold(f32::INFINITY, f32::min);
        layout.supports.push(BridgeSupport {
            start: a,
            end: b,
            bottom: (floor - FOOTING_EMBED).min(spring - 0.1),
        });
    }
    Ok(())
}

/// Fit the carriageway, masonry openings and foundations to a read-only height sampler.
///
/// # Errors
/// Returns an error for incompatible approaches, missing terrain, or insufficient
/// room to keep the road and main opening clear.
pub fn fit_bridge(
    request: &BridgeRequest,
    sample: impl Fn(Vec2) -> Option<f32>,
) -> Result<BridgeLayout, String> {
    fit_bridge_with_profile(request, BridgeSpanProfile::default(), sample)
}

/// Fit a bridge using the recipe's preferred clear span range.
///
/// # Errors
/// In addition to the fit errors from [`fit_bridge`], rejects invalid span ranges.
pub fn fit_bridge_with_profile(
    request: &BridgeRequest,
    profile: BridgeSpanProfile,
    sample: impl Fn(Vec2) -> Option<f32>,
) -> Result<BridgeLayout, String> {
    profile.validate()?;
    let mut layout = BridgeLayout::from_request(request)?;
    let channel = protected_channel(&layout)?;
    arrange_bays(&mut layout, channel, profile)?;
    fit_deck(&mut layout, &sample)?;
    fit_openings(&mut layout, channel, &sample)?;
    fit_supports(&mut layout, &sample)?;
    Ok(layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(length: f32) -> BridgeRequest {
        BridgeRequest {
            start: [0., 0., 5.],
            end: [length, 0., 5.],
            start_heading: None,
            end_heading: None,
            width: 3.8,
            kind: BridgeKind::Ravine,
            surface: BridgeSurface::CartTrack,
            channel: None,
        }
    }

    #[test]
    fn single_arch_stays_straight_between_unequal_diagonal_approaches() {
        let mut span = request(10.);
        span.start = [40., -20., 5.];
        span.end = [46., -12., 5.5];
        span.start_heading = Some([0.50, 0.86]);
        span.end_heading = Some([0.65, 0.76]);
        let fit = fit_bridge(&span, |_| Some(2.)).unwrap();
        assert_eq!(fit.bays.len(), 1);
        assert_eq!(fit.supports.len(), 2);
        for (station, end) in [(0., span.start), (fit.length, span.end)] {
            assert!(
                fit.point(station, 0.)
                    .extend(fit.deck_height(station))
                    .distance(Vec3::from_array(end))
                    < 0.0001
            );
        }
        for fraction in [0.25, 0.5, 0.75] {
            let station = fit.length * fraction;
            let expected = fit.start.truncate().lerp(fit.end.truncate(), fraction);
            assert!(fit.point(station, 0.).distance(expected) < 0.0001);
            assert!(fit.point(station, 1.).distance(expected + fit.axis.perp()) < 0.0001);
        }
        let bay = fit.bays[0];
        assert!((bay.intrados(bay.start) - bay.spring).abs() < 0.0001);
        assert!((bay.intrados(bay.end) - bay.spring).abs() < 0.0001);
        assert!((bay.intrados(bay.middle()) - bay.spring - bay.rise).abs() < 0.0001);
        assert!(bay.rise < (bay.end - bay.start) * 0.25);
        for s in stations(bay.start, bay.end, 0.03) {
            assert!(fit.deck_height(s) - bay.intrados(s) >= COVER - 1e-5);
        }
    }

    #[test]
    fn long_ravine_has_repeating_bays_and_footings_embedded_across_each_pier() {
        let span = request(70.);
        let ground = |p: Vec2| 1. + p.x * 0.015 - p.y * 0.17;
        let fit = fit_bridge(&span, |p| Some(ground(p))).unwrap();
        assert!(fit.bays.len() >= 6);
        assert_eq!(fit.supports.len(), fit.bays.len() + 1);
        for (i, bay) in fit.bays.iter().enumerate() {
            assert!((6. ..=10.).contains(&(bay.end - bay.start)));
            assert_eq!(fit.supports[i].end, bay.start);
            assert_eq!(fit.supports[i + 1].start, bay.end);
        }
        for support in &fit.supports {
            let half = span.width * 0.5 + WALL;
            let low_corner = ground(fit.point(support.start, half));
            assert!((support.bottom - (low_corner - FOOTING_EMBED)).abs() < 0.0002);
        }
    }

    #[test]
    fn skewed_off_center_channel_is_open_across_the_whole_pier_width() {
        let mut span = request(66.);
        span.kind = BridgeKind::River;
        let center = Vec2::new(25.3, 0.);
        let flow = Vec2::new(0.4, 0.9).normalize();
        span.channel = Some(BridgeChannel {
            center: center.to_array(),
            direction: flow.to_array(),
            half_width: 4.3,
            water_height: 3.8,
        });
        let fit = fit_bridge(&span, |_| Some(1.)).unwrap();
        let main = fit
            .bays
            .iter()
            .find(|bay| bay.start < center.x && bay.end > center.x)
            .unwrap();
        assert!(main.end - main.start >= 10.5);
        for lateral in [-span.width * 0.5 - WALL, 0., span.width * 0.5 + WALL] {
            let station = center.x + flow.x / flow.y * lateral;
            assert!(main.intrados(station) >= 3.8 + MIN_OPENING - 1e-4);
        }
        for support in &fit.supports {
            for s in [support.start, support.end] {
                for side in [-1., 1.] {
                    let corner = fit.point(s, side * (span.width * 0.5 + WALL));
                    assert!(flow.perp_dot(corner - center).abs() > 4.3);
                }
            }
        }
    }

    #[test]
    fn low_crossing_gets_a_crest_with_continuous_end_grades() {
        let span = request(42.);
        let fit = fit_bridge(&span, |_| Some(4.5)).unwrap();
        assert!(fit.crest > 0.);
        for s in stations(0., fit.length - 0.02, 0.05) {
            assert!(
                (fit.deck_height(s + 0.02) - fit.deck_height(s)).abs() / 0.02 < MAX_GRADE + 0.001
            );
        }
        assert!((fit.deck_height(0.02) - 5.).abs() < 1e-5);
        assert!((fit.deck_height(fit.length - 0.02) - 5.).abs() < 1e-5);
    }

    #[test]
    fn rejects_impossible_spans_missing_ground_and_blocked_waterways() {
        for length in [0., f32::NAN, 2.] {
            assert!(fit_bridge(&request(length), |_| Some(0.)).is_err());
        }
        let mut span = request(25.);
        span.end[2] += 4.;
        assert!(fit_bridge(&span, |_| Some(0.)).is_err());
        span.end[2] -= 4.;
        assert!(fit_bridge(&span, |_| None).is_err());
        assert!(
            fit_bridge(&span, |p| Some(if p.x > 10. && p.x < 15. {
                8.
            } else {
                2.
            }))
            .is_err()
        );
        span.channel = Some(BridgeChannel {
            center: [12., 0.],
            direction: [0., 1.],
            half_width: 2.,
            water_height: 7.,
        });
        assert!(fit_bridge(&span, |_| Some(1.)).is_err());
        span.channel.as_mut().unwrap().water_height = 1.;
        span.channel.as_mut().unwrap().half_width = 12.;
        assert!(fit_bridge(&span, |_| Some(1.)).is_err());
    }

    #[test]
    fn recipe_span_range_changes_bay_count_and_validates() {
        let span = request(86.);
        let normal = fit_bridge(&span, |_| Some(1.)).unwrap();
        let narrow = BridgeSpanProfile {
            min_clear_span: 3.,
            max_clear_span: 5.,
        };
        let fit = fit_bridge_with_profile(&span, narrow, |_| Some(1.)).unwrap();
        assert!(fit.bays.len() > normal.bays.len());
        assert!(
            fit.bays
                .iter()
                .all(|b| (3. ..=5.).contains(&(b.end - b.start)))
        );
        assert!(
            fit_bridge_with_profile(
                &span,
                BridgeSpanProfile {
                    min_clear_span: 7.,
                    ..narrow
                },
                |_| Some(1.)
            )
            .is_err()
        );
    }

    #[test]
    fn accepts_unset_unity_headings_and_rejects_malformed_vectors() {
        let mut json = serde_json::to_value(request(17.)).unwrap();
        for value in [
            serde_json::Value::Null,
            serde_json::json!([]),
            serde_json::json!([1., 0.]),
        ] {
            json["start_heading"] = value;
            assert!(serde_json::from_value::<BridgeRequest>(json.clone()).is_ok());
        }
        json["start_heading"] = serde_json::json!([1.]);
        assert!(serde_json::from_value::<BridgeRequest>(json).is_err());
    }
}

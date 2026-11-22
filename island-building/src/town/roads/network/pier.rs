use super::{Plan, RouteKind, StreetLayout, corridor, grade_options};
use crate::town::{TownError, TownInput, TownSettings, placement, roads};
use motu::{
    Vec2,
    piers::PierLayout,
    roads::{MAX_ROAD_EDGES, grade_roads},
};

impl StreetLayout {
    /// Add a complete straight street to the selected town component.
    pub fn with_pier(
        &self,
        root: usize,
        centre: Vec2,
        candidates: &[PierLayout],
        settings: &TownSettings,
        input: &TownInput,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Self>, TownError> {
        if candidates.is_empty() {
            return Ok(None);
        }
        let config = settings
            .roads
            .expect("pier street requires configured roads");
        let options = grade_options(settings, input);
        let mut ring_nodes = vec![None; self.plan.nodes.len()];
        for strip in &self.roads.strips {
            let section = self.plan.sections[self.edge_to_section[strip.source]];
            if self.components[strip.start] == self.components[root]
                && let Some(ring) = section.ring
            {
                ring_nodes[strip.start] = Some(ring);
                ring_nodes[strip.end] = Some(ring);
            }
        }
        let mut candidates: Vec<_> = candidates.iter().collect();
        candidates.sort_by(|a, b| {
            a.shore
                .distance_squared(centre)
                .total_cmp(&b.shore.distance_squared(centre))
        });
        let mut targets = Vec::new();
        for &pier in candidates {
            let landing = pier.start.truncate();
            if !pier.start.is_finite() || !pier.shore.is_finite() || !pier.axis.is_normalized() {
                continue;
            }
            targets.clear();
            targets.extend(ring_nodes.iter().enumerate().filter_map(|(node, &ring)| {
                let distance = self.plan.nodes[node].distance_squared(landing);
                ring.filter(|_| (4.0..=220_f32.powi(2)).contains(&distance))
                    .map(|ring| (distance, node, ring))
            }));
            targets.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let mut graded = 0;
            for &(distance, node, ring) in &targets {
                if cancelled() {
                    return Err(TownError::Cancelled);
                }
                let target = self.plan.nodes[node];
                let forward = (landing - target) / distance.sqrt();
                if forward.dot(pier.axis) < 0.35 || !self.clear_turn(node, forward) {
                    continue;
                }
                let reach = config.access_road_half_width() + options.shoulder;
                let road = corridor(target, landing, reach, 0.);
                let past_junction =
                    target + forward * (2. * reach + 0.5).min(distance.sqrt() * 0.5);
                if placement::protected(&road, input)
                    || placement::touches_river(&road, input, settings.river_margin)
                    || roads::obstructs(
                        &self.roads,
                        &corridor(past_junction, landing, reach, 0.),
                        false,
                    )
                    || ![0.25, 0.5, 0.75, 1.].into_iter().all(|t| {
                        let point = target.lerp(landing, t);
                        input
                            .terrain
                            .sample(point.x, point.y)
                            .is_some_and(|h| h >= options.min_height)
                    })
                {
                    continue;
                }
                if let Some(layout) = self.grade_pier(pier, node, ring, settings, input) {
                    return Ok(Some(layout));
                }
                graded += 1;
                if graded >= 8 {
                    break;
                }
            }
        }
        Ok(None)
    }

    fn clear_turn(&self, node: usize, forward: Vec2) -> bool {
        let here = self.plan.nodes[node];
        self.roads.strips.iter().all(|strip| {
            let neighbour = match (strip.start, strip.end) {
                (start, end) if start == node => end,
                (start, end) if end == node => start,
                _ => return true,
            };
            forward.dot((self.plan.nodes[neighbour] - here).normalize()) < 0.75
        })
    }

    fn grade_pier(
        &self,
        mut pier: PierLayout,
        target: usize,
        ring: usize,
        settings: &TownSettings,
        input: &TownInput,
    ) -> Option<Self> {
        // Own the trial topology so a rejected approach leaves the original network intact.
        let mut plan: Plan = self.plan.clone();
        let first = plan.sections.len();
        let half_width = settings.roads?.access_road_half_width();
        let landing = pier.start.truncate();
        plan.route(
            [plan.nodes[target], landing],
            half_width,
            RouteKind::Pier(ring),
        );
        let edge_to_section: Vec<_> = self
            .roads
            .strips
            .iter()
            .map(|strip| self.edge_to_section[strip.source])
            .chain(first..plan.sections.len())
            .collect();
        if edge_to_section.len() > MAX_ROAD_EDGES {
            return None;
        }
        let edges: Vec<_> = edge_to_section
            .iter()
            .map(|&index| plan.sections[index].edge)
            .collect();
        let roads = grade_roads(
            &plan.nodes,
            &edges,
            grade_options(settings, input),
            |point| {
                if point == landing {
                    Some(pier.start.z)
                } else {
                    input.terrain.sample(point.x, point.y)
                }
            },
        );
        if roads.strips.len() != edges.len() || roads.validate().is_err() {
            return None;
        }
        let height = roads.surface_height(landing)?;
        if (height - pier.start.z).abs() > 0.25 {
            return None;
        }
        pier.start.z = height;
        let mut components = self.components.clone();
        components.resize(plan.nodes.len(), self.components[target]);
        Some(Self {
            roads,
            plan,
            components,
            edge_to_section,
            bridges: self.bridges.clone(),
            pier: Some(pier),
        })
    }
}

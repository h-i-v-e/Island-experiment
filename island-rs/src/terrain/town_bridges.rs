//! Bridge terrain fitting against the resident solid mesh and river profiles.
use super::{Island, road_grade::MAXIMUM_SUBGRID_RAISE_METRES};
use crate::{
    ISLAND_WORLD_METRES, RiverNode, Vec2, Vec3,
    bridges::{
        BridgeChannel, BridgeKind, BridgeLayout, BridgeRequest, BridgeSpanProfile,
        fit_bridge_with_profile,
    },
    roads::RoadNetwork,
};

impl Island {
    pub(crate) fn fit_town_bridge(
        &self,
        mut request: BridgeRequest,
        profile: BridgeSpanProfile,
        impending_roads: Option<&RoadNetwork>,
    ) -> Result<BridgeLayout, String> {
        if request.kind == BridgeKind::River {
            request.channel = Some(
                self.bridge_channel(request.start, request.end)
                    .or(request.channel)
                    .ok_or("the river bridge does not cross a known channel")?,
            );
        }
        fit_bridge_with_profile(&request, profile, |point| {
            let natural = self
                .terrain
                .sample_accurate_surface(point / ISLAND_WORLD_METRES)
                .map(|hit| hit.position.z * ISLAND_WORLD_METRES)?;
            let graded = impending_roads.map_or(natural, |roads| {
                roads.terrain_support_height(point, natural, MAXIMUM_SUBGRID_RAISE_METRES)
            });
            Some(if natural > 0. && graded > 0. {
                graded
            } else {
                natural
            })
        })
    }

    pub(crate) fn bridge_channel(&self, start: [f32; 3], end: [f32; 3]) -> Option<BridgeChannel> {
        let start = Vec3::from_array(start).truncate();
        let span = Vec3::from_array(end).truncate() - start;
        let axis = span.try_normalize()?;
        let mut best = None::<(f32, BridgeChannel)>;
        for river in &self.rivers {
            let widths = river.target_half_widths(
                self.options.river_source_width_metres,
                self.options.river_maximum_width_metres,
            );
            for (nodes, widths) in river.nodes.windows(2).zip(widths.windows(2)) {
                let [a, b] = [&nodes[0], &nodes[1]];
                let Some(channel) = crossed_channel(start, span, [a, b], [widths[0], widths[1]])
                else {
                    continue;
                };
                let along_span =
                    channel.half_width / axis.perp_dot(Vec2::from_array(channel.direction)).abs();
                if best.is_none_or(|(prior, _)| along_span > prior) {
                    best = Some((along_span, channel));
                }
            }
        }
        best.map(|(_, channel)| channel)
    }

    #[cfg(test)]
    pub(crate) fn bridge_fixture() -> Self {
        use super::Terrain;
        use crate::{ForestOptions, Mesh, River};

        let mut island = Self::shared_forest_fixture(ForestOptions::default());
        let mut mesh = Mesh::default();
        for row in 0_u16..=16 {
            for column in 0_u16..=24 {
                let x = 600. + 2. * f32::from(column);
                let y = 636. + 2. * f32::from(row);
                let left_bank = ((616. - x) / 4.).clamp(0., 1.);
                let right_bank = ((x - 634.) / 4.).clamp(0., 1.);
                let height = 1.5 + left_bank * 8.5 + right_bank * 9.;
                mesh.vertices
                    .push(Vec3::new(x, y, height) / ISLAND_WORLD_METRES);
                if row > 0 && column > 0 {
                    let index = u32::from(row) * 25 + u32::from(column);
                    mesh.triangles.extend([
                        index - 26,
                        index - 25,
                        index,
                        index - 26,
                        index,
                        index - 1,
                    ]);
                }
            }
        }
        island.terrain = Terrain::new(mesh);
        island.rivers = vec![River {
            nodes: [636., 668.]
                .into_iter()
                .map(|y| RiverNode {
                    vertex: 0,
                    flow: 1,
                    surface: 3. / ISLAND_WORLD_METRES,
                    position: Vec3::new(625., y, 1.5) / ISLAND_WORLD_METRES,
                })
                .collect(),
            join: None,
        }];
        island
    }
}

fn crossed_channel(
    start: Vec2,
    span: Vec2,
    [first, last]: [&RiverNode; 2],
    [first_width, last_width]: [f32; 2],
) -> Option<BridgeChannel> {
    let source = first.position.truncate() * ISLAND_WORLD_METRES;
    let reach = last.position.truncate() * ISLAND_WORLD_METRES - source;
    let crossing = span.perp_dot(reach);
    if crossing.abs() < 1.0e-4 {
        return None;
    }
    let offset = source - start;
    let along_span = offset.perp_dot(reach) / crossing;
    let along_reach = offset.perp_dot(span) / crossing;
    let interval = 0. ..=1.;
    if !interval.contains(&along_span) || !interval.contains(&along_reach) {
        return None;
    }
    Some(BridgeChannel {
        center: (start + span * along_span).to_array(),
        direction: reach.normalize().to_array(),
        half_width: (last_width - first_width).mul_add(along_reach, first_width),
        water_height: ((last.surface - first.surface).mul_add(along_reach, first.surface)
            * ISLAND_WORLD_METRES)
            .max(0.),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_uses_world_river_surface_and_interpolated_width() {
        let node = |xy, water| RiverNode {
            vertex: 0,
            flow: 1,
            surface: water / ISLAND_WORLD_METRES,
            position: Vec3::new(120., xy, 1.) / ISLAND_WORLD_METRES,
        };
        let nodes = [node(90., 5.), node(110., 3.)];
        let start = Vec2::new(110., 100.);
        let channel =
            crossed_channel(start, Vec2::new(20., 0.), [&nodes[0], &nodes[1]], [2., 4.]).unwrap();
        assert!(Vec2::from_array(channel.center).distance(Vec2::new(120., 100.)) < 0.001);
        assert!(Vec2::from_array(channel.direction).distance(Vec2::Y) < 0.001);
        assert!((channel.half_width - 3.).abs() < 0.001);
        assert!((channel.water_height - 4.).abs() < 0.001);
        assert!(crossed_channel(start, Vec2::Y, [&nodes[0], &nodes[1]], [2., 4.]).is_none());
    }
}

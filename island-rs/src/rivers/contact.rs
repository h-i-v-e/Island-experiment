//! Physical channel contact changes receiver edges, never rainfall. Accumulate
//! once on the resulting forest after all confluence redirects are complete.
mod spatial;
#[cfg(test)]
mod tests;
use spatial::SegmentIndex;

use super::drainage::Drainage;
use super::tracing::{
    calculate_flow_and_catchment, drainage_order, find_sources, preferred_upstream,
    prioritize_inherited_sources,
};
use super::{
    Adjacency, Mesh, River, RiverChannelSettings, RiverGenerationSettings, RiverNode, Vec2,
};

#[derive(Clone, Copy)]
struct Owner {
    river: usize,
    half_width: f32,
    level: f32,
}

#[derive(Clone)]
struct Contact {
    path: Vec<usize>,
    nodes: usize,
    owner: Owner,
    distance: f32,
}

pub(super) struct ContactEnvironment<'a> {
    pub(super) mesh: &'a Mesh,
    pub(super) adjacency: &'a Adjacency,
    pub(super) ocean: &'a [bool],
    pub(super) excluded: &'a [bool],
    pub(super) flow: &'a [u32],
    pub(super) catchment: &'a [f32],
    pub(super) channel: RiverChannelSettings,
}

pub(super) struct ResolvedContacts {
    pub(super) drainage: Drainage,
    pub(super) flow: Vec<u32>,
    pub(super) catchment: Vec<f32>,
    pub(super) sources: Vec<usize>,
}

/// Accepted physical junctions are commitments for this solve. Replanning the
/// mutated forest without this state is a different routing problem. Each
/// changing pass commits a previously untouched receiver, giving finite work
/// while later Q/width updates may move contacts earlier on upstream vertices.
pub(super) struct ContactPlan {
    committed: Vec<bool>,
}

impl ContactPlan {
    pub(super) fn new(vertices: usize) -> Self {
        Self {
            committed: vec![false; vertices],
        }
    }
    pub(super) fn join(
        &mut self,
        environment: &ContactEnvironment<'_>,
        drainage: &mut Drainage,
        sources: &[usize],
    ) -> usize {
        join_with_plan(environment, drainage, sources, &mut self.committed)
    }
}

pub(super) fn resolve(
    mesh: &Mesh,
    adjacency: &Adjacency,
    settings: RiverGenerationSettings,
    ocean: &[bool],
    excluded: &[bool],
    inherited: &[Vec2],
    seed: u64,
) -> ResolvedContacts {
    let mut drainage = Drainage::new(mesh, adjacency, ocean, excluded, seed);
    let mut plan = ContactPlan::new(mesh.vertices.len());
    loop {
        let (flow, catchment) = calculate_flow_and_catchment(mesh, &drainage.downstream);
        let candidates = find_sources(
            mesh,
            adjacency,
            &drainage.downstream,
            &catchment,
            settings.source_rule,
        );
        let sources = prioritize_inherited_sources(
            mesh,
            adjacency,
            &candidates,
            inherited,
            excluded,
            &catchment,
        );
        let environment = ContactEnvironment {
            mesh,
            adjacency,
            ocean,
            excluded,
            flow: &flow,
            catchment: &catchment,
            channel: settings.channel_settings,
        };
        if plan.join(&environment, &mut drainage, &sources) == 0 {
            return ResolvedContacts {
                drainage,
                flow,
                catchment,
                sources,
            };
        }
        // Recompute true rainfall and configured widths once per pass. The
        // retained contact commitments prevent oscillation without river caps.
    }
}

struct Search<'a> {
    current: usize,
    half_width: f32,
    level: f32,
    owners: &'a [Option<Owner>],
    prefix: &'a [usize],
    stamp: usize,
    perimeter: &'a [bool],
    downstream: &'a [usize],
    committed: &'a [bool],
}

/// One-ring surfaces can touch across a shared vertex OR across an edge of a
/// mixed-owner wet triangle. The latter needs three centreline adjacency edges.
#[cfg(test)]
pub(super) fn join_contacts(
    environment: &ContactEnvironment<'_>,
    drainage: &mut Drainage,
    sources: &[usize],
) -> usize {
    ContactPlan::new(environment.mesh.vertices.len()).join(environment, drainage, sources)
}

fn join_with_plan(
    environment: &ContactEnvironment<'_>,
    drainage: &mut Drainage,
    sources: &[usize],
    committed: &mut [bool],
) -> usize {
    let order = drainage_order(&drainage.downstream);
    let upstream = preferred_upstream(
        drainage,
        environment.catchment,
        sources,
        environment.ocean,
        &order,
    );
    let mut levels: Vec<_> = environment
        .mesh
        .vertices
        .iter()
        .map(|vertex| vertex.z)
        .collect();
    for &vertex in order.iter().rev() {
        levels[vertex] = levels[vertex].max(levels[drainage.downstream[vertex]]);
    }
    let mut owners = vec![None; levels.len()];
    let mut prefix = vec![0_usize; levels.len()];
    let perimeter = environment.mesh.perimeter_mask();
    let mut contacts = 0;
    let mut segments = SegmentIndex::new(environment.channel.maximum_width);
    for (priority, &source) in sources.iter().enumerate() {
        if !drainage.reaches_ocean[source] || environment.ocean[source] || owners[source].is_some()
        {
            continue;
        }
        let mut start = source;
        while let Some(incoming) = upstream[start]
            .filter(|&vertex| owners[vertex].is_none() && drainage.downstream[vertex] == start)
        {
            start = incoming;
        }
        let river = environment.route(start, drainage, &owners, &levels);
        let widths = river.target_half_widths(
            environment.channel.source_width,
            environment.channel.maximum_width,
        );
        let stamp = priority + 1;
        let mut accepted = Vec::new();
        let mut changed_owners = Vec::new();
        for (index, (node, &half_width)) in river.nodes.iter().zip(&widths).enumerate() {
            let current = node.vertex;
            if environment.ocean[current] || owners[current].is_some() {
                break;
            }
            prefix[current] = stamp;
            accepted.push((current, half_width));
            let search = Search {
                current,
                half_width: half_width.max(widths.get(index + 1).copied().unwrap_or(half_width)),
                level: levels[current],
                owners: &owners,
                prefix: &prefix,
                stamp,
                perimeter: &perimeter,
                downstream: &drainage.downstream,
                committed,
            };
            let Some(contact) = segments.find_contact(environment, &search) else {
                continue;
            };
            if contact.owner.level <= levels[current] + 1.0e-6 {
                contacts += forward_contact(
                    &contact,
                    drainage,
                    &mut levels,
                    &mut accepted,
                    half_width,
                    committed,
                );
            } else {
                let (changed, newly_owned) = capture_lower_channel(
                    &contact,
                    drainage,
                    &mut levels,
                    &mut owners,
                    &mut prefix,
                    stamp,
                    committed,
                );
                contacts += changed;
                changed_owners.extend(newly_owned);
            }
            break;
        }
        for (vertex, half_width) in accepted {
            changed_owners.push(vertex);
            owners[vertex].get_or_insert(Owner {
                river: priority,
                half_width,
                level: levels[vertex],
            });
        }
        for vertex in changed_owners {
            segments.insert(environment, &owners, &drainage.downstream, vertex);
        }
    }
    contacts
}

fn forward_contact(
    contact: &Contact,
    drainage: &mut Drainage,
    levels: &mut [f32],
    accepted: &mut Vec<(usize, f32)>,
    width: f32,
    committed: &mut [bool],
) -> usize {
    let mut changed = 0;
    for edge in contact.path[..contact.nodes].windows(2) {
        changed += replace_receiver(drainage, edge[0], edge[1], committed);
    }
    let mut level = contact.owner.level;
    for &vertex in contact.path[1..contact.nodes - 1].iter().rev() {
        level = level.max(levels[vertex].min(levels[contact.path[0]]));
        levels[vertex] = level;
        accepted.push((vertex, width));
    }
    changed
}

/// The higher established trunk continues DOWN into the lower channel. Its
/// upstream grade is untouched. Release its abandoned segment only up to the
/// first common suffix so residual catchments can be considered independently.
fn capture_lower_channel(
    contact: &Contact,
    drainage: &mut Drainage,
    levels: &mut [f32],
    owners: &mut [Option<Owner>],
    prefix: &mut [usize],
    stamp: usize,
    committed: &mut [bool],
) -> (usize, Vec<usize>) {
    let mut changed_owners = Vec::new();
    let receiver = contact.path[0];
    let donor = contact.path[contact.nodes - 1];
    let old_next = drainage.downstream[donor];
    let mut suffix = Vec::new();
    let mut vertex = receiver;
    loop {
        prefix[vertex] = stamp;
        suffix.push(vertex);
        if drainage.downstream[vertex] == vertex {
            break;
        }
        vertex = drainage.downstream[vertex];
    }
    let mut old = old_next;
    while prefix[old] != stamp && drainage.downstream[old] != old {
        owners[old] = None;
        let next = drainage.downstream[old];
        if next == old {
            break;
        }
        old = next;
    }
    let mut changed = 0;
    for edge in contact.path[..contact.nodes].windows(2) {
        changed += replace_receiver(drainage, edge[1], edge[0], committed);
    }
    let mut level = levels[receiver];
    for &intermediate in &contact.path[1..contact.nodes - 1] {
        level = level.max(levels[intermediate].min(contact.owner.level));
        levels[intermediate] = level;
        suffix.push(intermediate);
    }
    for vertex in suffix {
        if drainage.downstream[vertex] != vertex
            && owners[vertex].is_none_or(|owner| owner.river >= contact.owner.river)
        {
            owners[vertex] = Some(Owner {
                level: levels[vertex],
                ..contact.owner
            });
            changed_owners.push(vertex);
        }
    }
    (changed, changed_owners)
}

impl ContactEnvironment<'_> {
    fn route(
        &self,
        start: usize,
        drainage: &Drainage,
        owners: &[Option<Owner>],
        levels: &[f32],
    ) -> River {
        let mut nodes = Vec::new();
        let mut vertex = start;
        loop {
            nodes.push(RiverNode {
                vertex,
                flow: self.flow[vertex],
                surface: levels[vertex],
                position: self.mesh.vertices[vertex],
            });
            if self.ocean[vertex] || owners[vertex].is_some() {
                break;
            }
            vertex = drainage.downstream[vertex];
        }
        River { nodes, join: None }
    }

    fn blocked(&self, vertex: usize, search: &Search<'_>) -> bool {
        self.ocean[vertex]
            || search.perimeter[vertex]
            || self.excluded.get(vertex).copied().unwrap_or(false)
    }

    fn consider(&self, search: &Search<'_>, contact: Contact, best: &mut Option<Contact>) {
        let donor_head = search.level.max(contact.owner.level);
        let target = contact.path[contact.nodes - 1];
        if contact.path[1..contact.nodes - 1]
            .iter()
            .any(|&vertex| self.mesh.vertices[vertex].z > donor_head + 1.0e-6)
        {
            return;
        }
        let (receiver, forbidden) = if contact.owner.level <= search.level + 1.0e-6 {
            (target, &contact.path[..contact.nodes - 1])
        } else {
            (search.current, &contact.path[1..contact.nodes])
        };
        let reversed = contact.owner.level > search.level + 1.0e-6;
        let blocked = contact.path[..contact.nodes].windows(2).any(|edge| {
            let (from, to) = if reversed {
                (edge[1], edge[0])
            } else {
                (edge[0], edge[1])
            };
            search.committed[from] && search.downstream[from] != to
        });
        if blocked || would_cycle(search.downstream, receiver, forbidden) {
            return;
        }
        let replace = best.as_ref().is_none_or(|old| {
            (contact.owner.river, contact.nodes) < (old.owner.river, old.nodes)
                || (contact.owner.river == old.owner.river
                    && contact.nodes == old.nodes
                    && contact.distance < old.distance)
        });
        if replace {
            *best = Some(contact);
        }
    }

    fn distance(&self, left: usize, right: usize) -> f32 {
        self.mesh.vertices[left]
            .truncate()
            .distance(self.mesh.vertices[right].truncate())
    }
}

fn replace_receiver(
    drainage: &mut Drainage,
    from: usize,
    to: usize,
    committed: &mut [bool],
) -> usize {
    if drainage.downstream[from] == to {
        return 0;
    }
    debug_assert!(!committed[from]);
    drainage.downstream[from] = to;
    committed[from] = true;
    1
}

fn would_cycle(downstream: &[usize], mut receiver: usize, forbidden: &[usize]) -> bool {
    for _ in downstream {
        if forbidden.contains(&receiver) {
            return true;
        }
        let next = downstream[receiver];
        if next == receiver {
            return false;
        }
        receiver = next;
    }
    true
}

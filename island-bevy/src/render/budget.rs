//! What one frame is actually asked to draw.
//!
//! Frustum culling and [`VisibilityRange`](bevy::camera::visibility::VisibilityRange)
//! both work per entity, and neither reports what it removed. The census here
//! is the other half of that: every entity that carries a [`BudgetItem`] is
//! counted twice a frame — once because it exists, once more if the culling
//! stages left it standing — so a capture can state how much of the island it
//! drew rather than how much of it was loaded.
//!
//! The vertex count rides on the component instead of being read back off the
//! mesh asset, because the census runs every frame over thousands of entities
//! and an asset lookup each would cost more than the answer is worth.

use bevy::{camera::visibility::VisibilitySystems, prelude::*};

/// Which population one counted entity belongs to. The three are counted apart
/// because they are culled by different means and answer different questions:
/// how much ground is in front of the camera, how many of the thousands of
/// small instances survived, and how many of the groups those instances hang
/// off were looked at at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetClass {
    Terrain,
    Scatter,
    /// A scatter group's parent: not drawn itself, but the thing that decides
    /// whether the instances under it are looked at at all.
    Group,
}

/// One entity's contribution to the census.
#[derive(Component, Clone, Copy, Debug)]
pub struct BudgetItem {
    pub class: BudgetClass,
    pub vertices: u32,
}

impl BudgetItem {
    #[must_use]
    pub const fn terrain(vertices: u32) -> Self {
        Self {
            class: BudgetClass::Terrain,
            vertices,
        }
    }

    #[must_use]
    pub const fn scatter(vertices: u32) -> Self {
        Self {
            class: BudgetClass::Scatter,
            vertices,
        }
    }

    #[must_use]
    pub const fn group() -> Self {
        Self {
            class: BudgetClass::Group,
            vertices: 0,
        }
    }
}

/// One population's entities and vertices, as they stand and as they are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Census {
    pub entities: u32,
    pub drawn_entities: u32,
    pub vertices: u64,
    pub drawn_vertices: u64,
}

impl Census {
    fn count(&mut self, item: BudgetItem, drawn: bool) {
        self.entities += 1;
        self.vertices += u64::from(item.vertices);
        if drawn {
            self.drawn_entities += 1;
            self.drawn_vertices += u64::from(item.vertices);
        }
    }

    #[must_use]
    pub const fn culled_entities(&self) -> u32 {
        self.entities - self.drawn_entities
    }

    /// What was drawn against what was resident, as one line of a log or a
    /// panel.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{} of {} entities drawn ({} culled), {} of {} vertices",
            self.drawn_entities,
            self.entities,
            self.culled_entities(),
            thousands(self.drawn_vertices),
            thousands(self.vertices)
        )
    }
}

/// What the last completed visibility pass left standing.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderBudget {
    pub terrain: Census,
    pub scatter: Census,
    pub groups: Census,
}

pub struct BudgetPlugin;

impl Plugin for BudgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderBudget>().add_systems(
            PostUpdate,
            take_census.after(VisibilitySystems::CheckVisibility),
        );
    }
}

/// Runs after the visibility stages have decided the frame, so what it reports
/// is what the render world is about to be handed.
fn take_census(mut budget: ResMut<RenderBudget>, items: Query<(&BudgetItem, &ViewVisibility)>) {
    let mut census = RenderBudget::default();
    for (item, visibility) in &items {
        let population = match item.class {
            BudgetClass::Terrain => &mut census.terrain,
            BudgetClass::Scatter => &mut census.scatter,
            BudgetClass::Group => &mut census.groups,
        };
        population.count(*item, visibility.get());
    }
    budget.set_if_neq(census);
}

/// A count with thin separators, because the numbers here run to millions and
/// are read rather than parsed.
fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(' ');
        }
        text.push(digit);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{BudgetItem, Census, thousands};

    #[test]
    fn counts_what_was_drawn_apart_from_what_exists() {
        let mut census = Census::default();
        census.count(BudgetItem::terrain(100), true);
        census.count(BudgetItem::terrain(250), false);
        assert_eq!(census.entities, 2);
        assert_eq!(census.drawn_entities, 1);
        assert_eq!(census.culled_entities(), 1);
        assert_eq!(census.vertices, 350);
        assert_eq!(census.drawn_vertices, 100);
    }

    #[test]
    fn groups_long_counts_in_threes() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_670_192), "1 670 192");
        assert_eq!(thousands(12_775), "12 775");
    }
}

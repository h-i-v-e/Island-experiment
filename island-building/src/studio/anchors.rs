use super::{StudioState, Tab};
use crate::{
    DecorationAnchor, MaterialSlot,
    interaction::{BuildingInstance, BuildingLodRoot, DoorLeaf},
    render::{bevy_to_native, native_to_bevy},
};
use bevy::{
    picking::mesh_picking::ray_cast::{
        MeshRayCast, MeshRayCastSettings, RayCastVisibility, RayMeshHit,
    },
    prelude::*,
};
use bevy_egui::egui::{self, Color32, PointerButton, Pos2, Stroke};

const PIN: Color32 = Color32::from_rgb(95, 220, 243);
const SELECTED_PIN: Color32 = Color32::from_rgb(255, 204, 114);
const BACKGROUND: Color32 = Color32::from_rgb(9, 22, 32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Placement {
    #[default]
    Select,
    Add,
    Move,
}

#[derive(Default)]
pub(super) struct Editor {
    selected: Option<String>,
    placement: Placement,
    rename: Option<(String, String)>,
    viewport_pressed: bool,
}

impl Editor {
    fn select(&mut self, id: String) {
        self.selected = Some(id);
        self.rename = None;
        self.placement = Placement::Select;
    }

    fn selected_index(&self, anchors: &[DecorationAnchor]) -> Option<usize> {
        let id = self.selected.as_deref()?;
        anchors.iter().position(|anchor| anchor.id == id)
    }
}

pub(super) fn inspector(
    ui: &mut egui::Ui,
    anchors: &mut Vec<DecorationAnchor>,
    editor: &mut Editor,
    ready: bool,
) {
    ui.heading("Decoration anchors");
    let selected = editor.selected_index(anchors);
    if selected.is_none() && editor.selected.is_some() {
        *editor = Editor::default();
    }
    ui.add_enabled_ui(ready, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Add anchor").clicked() {
                editor.placement = Placement::Add;
            }
            if ui
                .add_enabled(selected.is_some(), egui::Button::new("Move selected"))
                .clicked()
            {
                editor.placement = Placement::Move;
            }
        });
    });
    if editor.placement != Placement::Select && ui.small_button("Cancel placement").clicked() {
        editor.placement = Placement::Select;
    }
    ui.label(if ready {
        match editor.placement {
            Placement::Select => "Select an anchor here or on the building.",
            Placement::Add => "Click a building surface to place a new anchor.",
            Placement::Move => "Click a building surface to move the selected anchor.",
        }
    } else {
        "Waiting for the current building preview."
    });
    for (index, anchor) in anchors.iter().enumerate() {
        if ui
            .selectable_label(selected == Some(index), &anchor.label)
            .on_hover_text(&anchor.id)
            .clicked()
        {
            editor.select(anchor.id.clone());
        }
    }
    let Some(index) = editor.selected_index(anchors) else {
        return;
    };
    ui.separator();
    edit_anchor(ui, &mut anchors[index], editor);
    if ui.button("Delete anchor").clicked() {
        anchors.remove(index);
        *editor = Editor::default();
    }
}

fn edit_anchor(ui: &mut egui::Ui, anchor: &mut DecorationAnchor, editor: &mut Editor) {
    if editor
        .rename
        .as_ref()
        .is_none_or(|(original, _)| *original != anchor.label)
    {
        editor.rename = Some((anchor.label.clone(), anchor.label.clone()));
    }
    let (_, label) = editor.rename.as_mut().expect("label draft initialized");
    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(label)
                .id_salt(&anchor.id)
                .hint_text("Unity lookup label")
                .desired_width(180.)
                .char_limit(128),
        );
        let trimmed = label.trim();
        let can_rename = !trimmed.is_empty() && trimmed != anchor.label;
        let enter = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let apply = ui
            .add_enabled(can_rename, egui::Button::new("Rename"))
            .clicked();
        if can_rename && (apply || enter) {
            trimmed.clone_into(&mut anchor.label);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Rotation around surface normal");
        ui.add(
            egui::DragValue::new(&mut anchor.twist_degrees)
                .speed(1.)
                .suffix("°"),
        );
    });
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct Picking<'w, 's> {
    rays: MeshRayCast<'w, 's>,
    parents: Query<'w, 's, &'static ChildOf>,
    lods: Query<'w, 's, &'static BuildingLodRoot>,
    meshes: Query<'w, 's, &'static Mesh3d>,
    doors: Query<'w, 's, &'static DoorLeaf>,
    ground: Query<'w, 's, (), With<super::StudioGround>>,
}

impl Picking<'_, '_> {
    fn hit(
        &mut self,
        ray: Ray3d,
        root: Entity,
        instance: &BuildingInstance,
    ) -> Option<(bool, &RayMeshHit)> {
        let surface = |entity| {
            let Some(parent) = self.parents.get(entity).ok().map(ChildOf::parent) else {
                return false;
            };
            self.lods
                .get(parent)
                .is_ok_and(|lod| lod.building == root && lod.level == 0)
        };
        let filter = |entity| {
            if surface(entity) {
                // Masked straw detail allows the underlying roof surface to take the anchor.
                return !self.meshes.get(entity).is_ok_and(|mesh| {
                    instance.prototype.lods[0]
                        .iter()
                        .zip(&instance.prototype.source.lods[0].parts)
                        .any(|(part, source)| {
                            part.mesh.id() == mesh.id()
                                && (source.material.thatch_shell_layer().is_some()
                                    || source.material == MaterialSlot::ThatchEnd)
                        })
                });
            }
            self.ground.contains(entity)
                || self
                    .doors
                    .get(entity)
                    .is_ok_and(|door| door.building == root)
                || self.parents.get(entity).is_ok_and(|parent| {
                    self.doors
                        .get(parent.parent())
                        .is_ok_and(|door| door.building == root)
                })
        };
        let settings = MeshRayCastSettings::default()
            .with_visibility(RayCastVisibility::Visible)
            .with_filter(&filter);
        self.rays
            .cast_ray(ray, &settings)
            .first()
            .map(|(entity, hit)| (surface(*entity), hit))
    }
}

struct Viewport<'a> {
    camera: &'a Camera,
    transform: &'a GlobalTransform,
    screen: egui::Rect,
    size: Vec2,
}

impl Viewport<'_> {
    fn project(&self, point: Vec3) -> Option<Pos2> {
        let screen = self.camera.world_to_viewport(self.transform, point).ok()? / self.size;
        Some(
            self.screen.min
                + egui::vec2(
                    screen.x * self.screen.width(),
                    screen.y * self.screen.height(),
                ),
        )
    }

    fn ray(&self, point: Pos2) -> Option<Ray3d> {
        let offset = point - self.screen.min;
        let position = Vec2::new(
            offset.x / self.screen.width(),
            offset.y / self.screen.height(),
        );
        self.camera
            .viewport_to_world(self.transform, position * self.size)
            .ok()
    }
}

struct Marker {
    pin: Pos2,
    normal: Option<Pos2>,
    forward: Option<Pos2>,
}

impl Marker {
    fn project(viewport: &Viewport, point: Vec3, normal: Vec3, forward: Vec3) -> Option<Self> {
        let size = viewport.transform.translation().distance(point) * 0.026;
        Some(Self {
            pin: viewport.project(point)?,
            normal: viewport.project(point + normal * size),
            forward: viewport.project(point + forward * size),
        })
    }

    fn paint(&self, painter: &egui::Painter, color: Color32, label: &str) -> egui::Rect {
        if let Some(normal) = self.normal {
            painter.line_segment([self.pin, normal], Stroke::new(4., BACKGROUND));
            painter.line_segment([self.pin, normal], Stroke::new(2., Color32::WHITE));
        }
        if let Some(forward) = self.forward {
            let arrow = forward - self.pin;
            if arrow.length_sq() > 1. {
                painter.arrow(self.pin, arrow, Stroke::new(4., BACKGROUND));
                painter.arrow(self.pin, arrow, Stroke::new(2., color));
            }
        }
        painter.circle(self.pin, 5., color, Stroke::new(2., BACKGROUND));
        let pin_rect = egui::Rect::from_center_size(self.pin, egui::vec2(18., 18.));
        if label.is_empty() {
            return pin_rect;
        }
        let text = painter.layout_no_wrap(label.to_owned(), egui::FontId::proportional(13.), color);
        let position = self.pin + egui::vec2(10., -text.size().y * 0.5);
        let label_rect =
            egui::Rect::from_min_size(position, text.size()).expand2(egui::vec2(4., 2.));
        painter.rect_filled(label_rect, 4., BACKGROUND);
        painter.galley(position, text, color);
        pin_rect.union(label_rect)
    }
}

struct Surface {
    point: Vec3,
    normal: Vec3,
}

impl Surface {
    fn from_hit(hit: &RayMeshHit) -> Option<Self> {
        let [a, b, c] = hit.triangle?;
        Some(Self {
            point: hit.point,
            normal: (b - a).cross(c - a).try_normalize()?,
        })
    }

    fn apply(&self, anchor: &mut DecorationAnchor, root: &GlobalTransform) {
        let inverse = root.affine().inverse();
        anchor.position = bevy_to_native(inverse.transform_point3(self.point));
        anchor.normal = bevy_to_native(inverse.transform_vector3(self.normal).normalize());
    }
}

pub(super) fn viewport(
    ui: &egui::Ui,
    state: &mut StudioState,
    camera: (&Camera, &GlobalTransform),
    preview: Option<(&BuildingInstance, &GlobalTransform)>,
    picking: &mut Picking,
    blocked: bool,
) {
    if state.tab != Tab::Decorations {
        state.anchors.placement = Placement::Select;
        state.anchors.viewport_pressed = false;
        return;
    }
    let (Some(root), Some((instance, transform)), Some(size)) =
        (state.root, preview, camera.0.logical_viewport_size())
    else {
        return;
    };
    if instance.current_lod != 0 {
        return;
    }
    let viewport = Viewport {
        camera: camera.0,
        transform: camera.1,
        screen: ui.ctx().viewport_rect(),
        size,
    };
    let rect = ui.available_rect_before_wrap();
    let painter = ui.painter().with_clip_rect(rect);
    let (hover, click) = viewport_pointer(ui.ctx(), rect, &mut state.anchors, blocked);
    let hovered = paint_anchors(
        &viewport, &painter, state, root, instance, transform, picking, hover,
    );
    if state.anchors.placement == Placement::Select {
        if click && let Some(index) = hovered {
            state
                .anchors
                .select(state.document.recipe().decoration_anchors[index].id.clone());
        }
        return;
    }
    if !state.decoration_preview_ready() {
        return;
    }
    let Some((true, hit)) = hover
        .and_then(|point| viewport.ray(point))
        .and_then(|ray| picking.hit(ray, root, instance))
    else {
        return;
    };
    if let Some(surface) = Surface::from_hit(hit) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        if click {
            place_anchor(state, &surface, transform);
        } else {
            paint_candidate(&viewport, &painter, state, &surface, transform);
        }
    }
}

fn viewport_pointer(
    context: &egui::Context,
    viewport: egui::Rect,
    editor: &mut Editor,
    blocked: bool,
) -> (Option<Pos2>, bool) {
    let (position, pressed, released, clicked, orbiting) = context.input(|input| {
        let pointer = &input.pointer;
        (
            pointer.interact_pos(),
            pointer.button_pressed(PointerButton::Primary),
            pointer.button_released(PointerButton::Primary),
            pointer.button_clicked(PointerButton::Primary),
            pointer.secondary_down(),
        )
    });
    let position = position.filter(|&point| {
        !blocked
            && !orbiting
            && viewport.contains(point)
            && !context.egui_is_using_pointer()
            && context
                .layer_id_at(point)
                .is_none_or(|layer| layer.order == egui::Order::Background)
    });
    if pressed {
        editor.viewport_pressed = position.is_some();
    }
    let click =
        released && std::mem::take(&mut editor.viewport_pressed) && clicked && position.is_some();
    (position, click)
}

fn paint_anchors(
    viewport: &Viewport,
    painter: &egui::Painter,
    state: &StudioState,
    root: Entity,
    instance: &BuildingInstance,
    transform: &GlobalTransform,
    picking: &mut Picking,
    hover: Option<Pos2>,
) -> Option<usize> {
    let anchors = &state.document.recipe().decoration_anchors;
    let selected = state.anchors.selected_index(anchors);
    let mut nearest: Option<(usize, f32)> = None;
    // Draw the selected marker last so its label remains readable at shared positions.
    for index in (0..anchors.len())
        .filter(|index| Some(*index) != selected)
        .chain(selected)
    {
        let anchor = &anchors[index];
        let point = transform.transform_point(native_to_bevy(anchor.position));
        let Some(marker) = display_marker(viewport, painter, anchor, transform, point) else {
            continue;
        };
        let Some(ray) = viewport.ray(marker.pin) else {
            continue;
        };
        if picking
            .hit(ray, root, instance)
            .is_some_and(|(_, hit)| hit.distance + 0.02 < ray.origin.distance(point))
        {
            continue;
        }
        let color = if Some(index) == selected {
            SELECTED_PIN
        } else {
            PIN
        };
        let hit_box = marker.paint(painter, color, &anchor.label);
        if let Some(pointer) = hover.filter(|&point| hit_box.contains(point)) {
            let distance = pointer.distance_sq(marker.pin);
            if nearest.is_none_or(|(_, best)| distance <= best) {
                nearest = Some((index, distance));
            }
        }
    }
    nearest.map(|(index, _)| index)
}

fn display_marker(
    viewport: &Viewport,
    painter: &egui::Painter,
    anchor: &DecorationAnchor,
    transform: &GlobalTransform,
    point: Vec3,
) -> Option<Marker> {
    let vector = |native| {
        transform
            .affine()
            .transform_vector3(native_to_bevy(native))
            .normalize_or_zero()
    };
    let marker = Marker::project(
        viewport,
        point,
        vector(anchor.unit_normal()?.to_array()),
        vector(anchor.forward()),
    )?;
    painter.clip_rect().contains(marker.pin).then_some(marker)
}

fn paint_candidate(
    viewport: &Viewport,
    painter: &egui::Painter,
    state: &StudioState,
    surface: &Surface,
    transform: &GlobalTransform,
) {
    let twist = state
        .anchors
        .selected_index(&state.document.recipe().decoration_anchors)
        .filter(|_| state.anchors.placement == Placement::Move)
        .map_or(0., |index| {
            state.document.recipe().decoration_anchors[index].twist_degrees
        });
    let mut candidate = DecorationAnchor {
        id: String::new(),
        label: String::new(),
        position: [0.; 3],
        normal: [0.; 3],
        twist_degrees: twist,
    };
    surface.apply(&mut candidate, transform);
    if let Some(marker) = display_marker(viewport, painter, &candidate, transform, surface.point) {
        marker.paint(painter, SELECTED_PIN, "");
    }
}

fn place_anchor(state: &mut StudioState, surface: &Surface, transform: &GlobalTransform) {
    let selected = state
        .anchors
        .selected_index(&state.document.recipe().decoration_anchors);
    match state.anchors.placement {
        Placement::Add => {
            let anchors = &state.document.recipe().decoration_anchors;
            let id = (state.document.revision()..)
                .take(anchors.len() + 1)
                .map(|number| format!("anchor-{number}"))
                .find(|id| !anchors.iter().any(|anchor| anchor.id == *id))
                .expect("unused anchor id");
            let label = (1..)
                .take(anchors.len() + 1)
                .map(|number| format!("Anchor {number}"))
                .find(|label| !anchors.iter().any(|anchor| anchor.label == *label))
                .expect("unused anchor label");
            let mut anchor = DecorationAnchor {
                id: id.clone(),
                label,
                position: [0.; 3],
                normal: [0.; 3],
                twist_degrees: 0.,
            };
            surface.apply(&mut anchor, transform);
            state
                .document
                .edit(|recipe| recipe.decoration_anchors.push(anchor));
            state.anchors.select(id);
        }
        Placement::Move => {
            if let Some(index) = selected {
                state
                    .document
                    .edit(|recipe| surface.apply(&mut recipe.decoration_anchors[index], transform));
            }
            state.anchors.placement = Placement::Select;
        }
        Placement::Select => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BuildingRecipe, document::StudioDocument, render::compile_building, studio::Settings,
    };
    use bevy::{
        camera::primitives::MeshAabb,
        ecs::system::SystemState,
        tasks::{ComputeTaskPool, TaskPool},
    };
    use std::sync::Arc;

    #[test]
    fn picks_generated_lod_zero_surfaces_with_native_normals() {
        ComputeTaskPool::get_or_init(TaskPool::new);
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 128;
        let prototype = Arc::new(crate::generate(&recipe).unwrap());
        let mut meshes = Assets::<Mesh>::default();
        let compiled = compile_building(
            prototype,
            &mut meshes,
            &mut Assets::default(),
            &mut Assets::default(),
        );
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let lod = world
            .spawn(BuildingLodRoot {
                building: root,
                level: 0,
            })
            .id();
        for part in &compiled.lods[0] {
            if let Some(bounds) = meshes.get(&part.mesh).and_then(MeshAabb::compute_aabb) {
                world.spawn((
                    Mesh3d(part.mesh.clone()),
                    bounds,
                    ChildOf(lod),
                    GlobalTransform::IDENTITY,
                    InheritedVisibility::VISIBLE,
                    ViewVisibility::default(),
                ));
            }
        }
        world.insert_resource(meshes);
        let instance = BuildingInstance::new(compiled, "test".into());
        let mut system = SystemState::<Picking>::new(&mut world);
        let mut picking = system.get(&world).unwrap();
        for (location, direction, facing) in [
            ([1.123, -0.521, 7.], Vec3::NEG_Z, Vec3::Z),
            ([1.123, -0.521, 1.7], Vec3::Z, Vec3::NEG_Z),
            ([1.123, -0.521, 1.7], Vec3::NEG_Z, Vec3::Z),
            ([1.123, -6., 1.7], Vec3::Y, Vec3::NEG_Y),
        ] {
            let ray = Ray3d::new(
                native_to_bevy(location),
                Dir3::new(native_to_bevy(direction.to_array())).unwrap(),
            );
            let (mountable, hit) = picking.hit(ray, root, &instance).unwrap();
            assert!(mountable);
            let surface = Surface::from_hit(hit).unwrap();
            assert!(surface.point.abs_diff_eq(ray.get_point(hit.distance), 1e-5));
            assert!(Vec3::from_array(bevy_to_native(surface.normal)).dot(facing) > 0.8);
        }
    }

    #[test]
    fn surface_edits_preserve_identity_and_history_without_rebuilding_the_preview() {
        let mut state = StudioState::new(
            StudioDocument::new(BuildingRecipe::hut()),
            &Settings::default(),
        );
        state.anchors.placement = Placement::Add;
        let root = GlobalTransform::IDENTITY;
        let wall = Surface {
            point: Vec3::new(1., 2., 3.),
            normal: Vec3::Z,
        };
        place_anchor(&mut state, &wall, &root);
        assert_eq!(
            state.document.recipe().decoration_anchors[0].position,
            [1., -3., 2.]
        );
        let placed = state.document.recipe().decoration_anchors[0].clone();
        let previous_export_revision = state.export_revision();
        let previous_preview_serial = state.serial;
        state
            .document
            .edit(|recipe| recipe.decoration_anchors[0].twist_degrees = 37.);
        state.anchors.placement = Placement::Move;
        let ceiling = Surface {
            point: Vec3::new(1.5, 4., 3.),
            normal: Vec3::NEG_Y,
        };
        place_anchor(&mut state, &ceiling, &root);
        state.sync_preview();
        let moved = &state.document.recipe().decoration_anchors[0];
        assert_eq!(
            (moved.id.as_str(), moved.label.as_str()),
            (placed.id.as_str(), placed.label.as_str())
        );
        assert_eq!(moved.normal, [0., 0., -1.]);
        assert_eq!(moved.twist_degrees.to_bits(), 37_f32.to_bits());
        assert_eq!(state.serial, previous_preview_serial);
        assert_ne!(state.export_revision(), previous_export_revision);
        state.document.undo();
        assert_eq!(
            state.document.recipe().decoration_anchors[0].position,
            placed.position
        );
        state.document.undo();
        assert_eq!(state.document.recipe().decoration_anchors[0], placed);
        state.document.undo();
        assert!(state.document.recipe().decoration_anchors.is_empty());
        state.document.redo();
        assert_eq!(state.document.recipe().decoration_anchors[0], placed);
    }

    #[test]
    fn clicks_need_both_press_and_release_inside_the_unblocked_viewport() {
        let context = egui::Context::default();
        let screen = egui::Rect::from_min_max(Pos2::ZERO, egui::pos2(800., 500.));
        let viewport = egui::Rect::from_min_max(egui::pos2(250., 100.), screen.max);
        let mut editor = Editor::default();
        let mut pointer = |position, pressed, blocked| {
            let event = egui::Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let mut result = false;
            let _ = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events: vec![egui::Event::PointerMoved(position), event],
                    ..Default::default()
                },
                |ui| result = viewport_pointer(ui.ctx(), viewport, &mut editor, blocked).1,
            );
            result
        };
        let off = egui::pos2(248., 150.);
        let on = egui::pos2(251., 150.);
        assert!(!pointer(off, true, false));
        assert!(!pointer(on, false, false));
        assert!(!pointer(on, true, false));
        assert!(!pointer(on, false, true));
        assert!(!pointer(on, true, true));
        assert!(!pointer(on, false, false));
        assert!(!pointer(on, true, false));
        assert!(pointer(on, false, false));
    }
}

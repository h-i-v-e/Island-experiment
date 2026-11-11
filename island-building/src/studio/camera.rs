use super::{StudioCamera, StudioObserver, StudioState, StudioSun};
use crate::{
    BuildingPrototype,
    interaction::{BuildingInstance, BuildingObserver, LodMode},
    render::native_to_bevy,
};
use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    prelude::*,
};
use bevy_egui::input::EguiWantsInput;

#[derive(Resource)]
pub(super) struct CameraControls {
    pub walkthrough: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    pub feet: Vec3,
}
impl Default for CameraControls {
    fn default() -> Self {
        Self {
            walkthrough: false,
            yaw: 0.65,
            pitch: 0.42,
            distance: 17.,
            target: Vec3::new(0., 1.5, 0.),
            feet: Vec3::new(0., 0.02, 5.),
        }
    }
}

impl CameraControls {
    pub fn frame(&mut self, prototype: &BuildingPrototype, view: &str) {
        let recipe = &prototype.recipe;
        self.walkthrough = false;
        self.target = Vec3::new(0., recipe.wall_height * 0.5, 0.);
        self.distance = recipe.width.max(recipe.depth) * 2.25;
        if recipe.chimney.is_some() {
            let min = native_to_bevy(prototype.layout.bounds_min);
            let max = native_to_bevy(prototype.layout.bounds_max);
            self.target = (min + max) * 0.5;
            let radius = min.distance(max) * 0.5;
            self.distance = self
                .distance
                .max(radius / 22.5_f32.to_radians().sin() * 1.1);
        }
        self.pitch = 0.36;
        self.yaw = match view {
            "south" => 0.,
            "north" => std::f32::consts::PI,
            "east" => std::f32::consts::FRAC_PI_2,
            "west" => -std::f32::consts::FRAC_PI_2,
            _ => 0.65,
        };
        if view == "entrance" {
            self.target = native_to_bevy(prototype.layout.entrance) + Vec3::Y * 1.2;
            self.distance = 5.5;
            self.pitch = 0.1;
            let entrance = prototype.layout.entrance;
            self.yaw = entrance[0].atan2(-entrance[1]);
        }
        if view == "interior" {
            self.walkthrough = true;
            let partitioned = !recipe.partitions.is_empty();
            self.feet = native_to_bevy([
                recipe.width * 0.5 - 0.65,
                if partitioned {
                    -recipe.depth * 0.5 + 0.65
                } else {
                    recipe.depth * 0.5 - 0.65
                },
                0.025,
            ]);
            // Review the room from its opposite corner: floor, ceiling,
            // entrance and side glazing remain visible together.
            let target = native_to_bevy([
                -recipe.width * 0.1,
                -recipe.depth * if partitioned { 0.1 } else { 0.3 },
                1.25,
            ]);
            let look = target - (self.feet + Vec3::Y * 1.65);
            self.yaw = (-look.x).atan2(-look.z);
            self.pitch = (-look.y / look.length()).asin();
        }
        if view == "fireplace" && !self.frame_fireplace(prototype) {
            self.frame(prototype, "interior");
        }
        if matches!(view, "roof-end" | "roof-under") {
            let x = if recipe.roof.kind == crate::RoofKind::Gable {
                0.
            } else {
                recipe.width * 0.5
            };
            self.target = native_to_bevy([
                x,
                -recipe.depth * 0.5 - recipe.roof.overhang,
                crate::geometry::chimney::roof_height(recipe, x)
                    - recipe.roof.effective_thickness()
                    - 0.35,
            ]);
            self.distance = 3.3;
            self.pitch = if view == "roof-under" { -0.30 } else { 0.04 };
            self.yaw = 0.22;
        }
        if view == "eaves" {
            self.frame_eaves(prototype);
        }
        if view == "window" {
            self.frame_window(prototype);
        }
    }

    fn frame_window(&mut self, prototype: &BuildingPrototype) {
        let recipe = &prototype.recipe;
        let Some(opening) = recipe
            .openings
            .iter()
            .find(|o| o.kind == crate::OpeningKind::Window)
        else {
            return;
        };
        self.target = native_to_bevy(opening.facade.point(
            recipe,
            opening.offset,
            opening.sill + opening.height * 0.5,
            recipe.wall_thickness * 0.48,
        ));
        self.distance = ((opening.width.max(opening.height) + 0.2) * 2.3).max(1.8);
        let normal = opening.facade.normal();
        self.yaw = normal[0].atan2(-normal[1]) + 0.28;
        self.pitch = 0.06;
    }

    fn frame_eaves(&mut self, prototype: &BuildingPrototype) {
        let recipe = &prototype.recipe;
        let eye_height = 1.65;
        let top = recipe.wall_height + recipe.roof.rise + recipe.roof.effective_thickness() + 0.22;
        self.target = native_to_bevy([
            0.,
            -recipe.depth * 0.5 - recipe.roof.overhang,
            recipe.wall_height + recipe.roof.rise * 0.35,
        ]);
        self.distance = (recipe.width.max(recipe.depth) * 2.).max((top - eye_height) * 2.4);
        self.pitch = ((eye_height - self.target.y) / self.distance).asin();
        self.yaw = 0.20;
    }

    fn frame_fireplace(&mut self, prototype: &BuildingPrototype) -> bool {
        let recipe = &prototype.recipe;
        let Some(chimney) = &recipe.chimney else {
            return false;
        };
        let Some(target) = crate::geometry::chimney::fireplace_centre(recipe) else {
            return false;
        };
        let position = [1.6, 1.3, 1.0, 0.75, 0.45]
            .into_iter()
            .find_map(|distance| {
                [0.0, 0.45, -0.45]
                    .into_iter()
                    .map(|shift| {
                        chimney.facade.point(
                            recipe,
                            chimney.offset + shift,
                            0.025,
                            recipe.wall_thickness + chimney.depth + chimney.hearth_depth + distance,
                        )
                    })
                    .find(|point| inspection_position_clear(prototype, *point))
            });
        let Some(position) = position else {
            return false;
        };
        self.walkthrough = true;
        self.feet = native_to_bevy(position);
        self.target = native_to_bevy(target);
        let look = self.target - (self.feet + Vec3::Y * 1.65);
        self.yaw = (-look.x).atan2(-look.z);
        self.pitch = (-look.y / look.length()).asin();
        true
    }

    pub fn enter_walkthrough(&mut self, prototype: &BuildingPrototype) {
        self.frame(prototype, "entrance");
        let entrance = native_to_bevy(prototype.layout.entrance);
        let outward = Vec3::new(entrance.x, 0., entrance.z).normalize_or_zero();
        self.feet = entrance + outward * 2.0 + Vec3::Y * 0.025;
        let direction = -outward;
        self.yaw = (-direction.x).atan2(-direction.z);
        self.pitch = 0.;
        self.walkthrough = true;
    }
}

fn inspection_position_clear(prototype: &BuildingPrototype, feet: [f32; 3]) -> bool {
    let recipe = &prototype.recipe;
    feet[0].abs() <= recipe.width * 0.5 - recipe.wall_thickness - 0.28
        && feet[1].abs() <= recipe.depth * 0.5 - recipe.wall_thickness - 0.28
        && prototype
            .colliders
            .iter()
            .all(|shape| !shape.intersects_capsule(feet, 0.28, 1.8))
        && prototype
            .doors
            .iter()
            .all(|door| !door.collider_at(0.0).intersects_capsule(feet, 0.28, 1.8))
}

#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub(super) fn update_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    wants: Option<Res<EguiWantsInput>>,
    mut controls: ResMut<CameraControls>,
    mut state: ResMut<StudioState>,
    mut camera: Single<&mut Transform, With<StudioCamera>>,
    mut instances: Query<(&mut BuildingInstance, &GlobalTransform)>,
    mut sun: Single<&mut DirectionalLight, With<StudioSun>>,
    mut ambient: ResMut<bevy::light::GlobalAmbientLight>,
    mut projection: Single<&mut Projection, With<StudioCamera>>,
) {
    if let Projection::Perspective(perspective) = projection.as_mut() {
        perspective.fov = if controls.walkthrough {
            75_f32.to_radians()
        } else {
            45_f32.to_radians()
        };
    }
    let allow_pointer = wants.as_ref().is_none_or(|w| !w.wants_any_pointer_input());
    let allow_keyboard = wants.as_ref().is_none_or(|w| !w.wants_any_keyboard_input());
    if allow_pointer && buttons.pressed(MouseButton::Right) {
        controls.yaw -= motion.delta.x * 0.005;
        controls.pitch = (controls.pitch + motion.delta.y * 0.005).clamp(-1.35, 1.35);
    }
    if !controls.walkthrough && allow_pointer {
        controls.distance = (controls.distance * (-scroll.delta.y * 0.09).exp()).clamp(1.4, 180.);
    }
    if controls.walkthrough && allow_keyboard {
        let forward = Vec3::new(-controls.yaw.sin(), 0., -controls.yaw.cos());
        let right = forward.cross(Vec3::Y);
        let axis = |positive, negative| {
            f32::from(u8::from(keys.pressed(positive)))
                - f32::from(u8::from(keys.pressed(negative)))
        };
        let movement = (forward * axis(KeyCode::KeyW, KeyCode::KeyS)
            + right * axis(KeyCode::KeyD, KeyCode::KeyA))
        .normalize_or_zero();
        let delta = movement
            * time.delta_secs().min(0.05)
            * if keys.pressed(KeyCode::ShiftLeft) {
                5.2
            } else {
                2.8
            };
        // Small substeps and axis sliding prevent tunnelling through thin door leaves.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // Clamped to 1..=8 substeps.
        let steps = (delta.length() / 0.07).ceil().clamp(1., 8.) as u8;
        for _ in 0..steps {
            for step in [
                Vec3::X * delta.x / f32::from(steps),
                Vec3::Z * delta.z / f32::from(steps),
            ] {
                let next = controls.feet + step;
                let blocked = instances
                    .iter()
                    .any(|(instance, root)| instance.blocks_capsule(root, next, 0.28, 1.8));
                if !blocked {
                    controls.feet = next;
                }
            }
        }
        if keys.just_pressed(KeyCode::Escape) {
            controls.walkthrough = false;
        }
    }
    let rotation = Quat::from_rotation_y(controls.yaw) * Quat::from_rotation_x(-controls.pitch);
    if controls.walkthrough {
        camera.translation = controls.feet + Vec3::Y * 1.65;
        camera.rotation = rotation;
    } else {
        let outward = Vec3::new(
            controls.yaw.sin() * controls.pitch.cos(),
            controls.pitch.sin(),
            controls.yaw.cos() * controls.pitch.cos(),
        );
        camera.translation = controls.target + outward * controls.distance;
        camera.look_at(controls.target, Vec3::Y);
    }
    for (mut instance, _) in &mut instances {
        instance.lod_mode = if state.automatic {
            LodMode::Auto
        } else {
            match state.lod {
                1 => LodMode::Lod1,
                2 => LodMode::Lod2,
                _ => LodMode::Lod0,
            }
        };
        if let Some(open) = state.door_command.take() {
            let targets: Vec<_> = instance
                .prototype
                .source
                .doors
                .iter()
                .map(|door| (door.id.clone(), if open { door.swing_angle } else { 0. }))
                .collect();
            for (id, target) in targets {
                if let Some(door) = instance.doors.get_mut(&id) {
                    door.target = target;
                }
            }
        }
    }
    sun.illuminance = state.light;
    ambient.brightness = state.ambient;
}

#[allow(clippy::needless_pass_by_value, clippy::type_complexity)] // Bevy component/filter query.
pub(super) fn sync_observer(
    camera: Single<&Transform, With<StudioCamera>>,
    mut observer: Single<
        (&mut Transform, &mut GlobalTransform, &mut BuildingObserver),
        (With<StudioObserver>, Without<StudioCamera>),
    >,
    keys: Res<ButtonInput<KeyCode>>,
    controls: Res<CameraControls>,
    wants: Option<Res<EguiWantsInput>>,
    projection: Single<&Projection, With<StudioCamera>>,
) {
    observer.0.translation = camera.translation - Vec3::Y * 1.65;
    *observer.1 = GlobalTransform::from(*observer.0);
    observer.2.view_direction = *camera.forward();
    if let Projection::Perspective(perspective) = &**projection {
        observer.2.vertical_fov_radians = perspective.fov;
    }
    observer.2.interaction_enabled =
        controls.walkthrough && wants.as_ref().is_none_or(|w| !w.wants_any_keyboard_input());
    observer.2.interact = controls.walkthrough
        && keys.just_pressed(KeyCode::KeyE)
        && wants.as_ref().is_none_or(|w| !w.wants_any_keyboard_input());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingRecipe, generate, render::bevy_to_native};

    #[test]
    fn fireplace_view_is_inside_clear_of_hearth_and_aims_at_the_firebox() {
        for mut recipe in [
            BuildingRecipe::hut(),
            BuildingRecipe::cottage(),
            BuildingRecipe::workshop(),
        ] {
            recipe.atlas_size = 128;
            let prototype = generate(&recipe).unwrap();
            let mut controls = CameraControls::default();
            controls.frame(&prototype, "fireplace");
            assert!(controls.walkthrough);
            assert!(inspection_position_clear(
                &prototype,
                bevy_to_native(controls.feet)
            ));
            let target =
                native_to_bevy(crate::geometry::chimney::fireplace_centre(&recipe).unwrap());
            assert!(controls.target.abs_diff_eq(target, 1e-5));
            let look = Quat::from_rotation_y(controls.yaw)
                * Quat::from_rotation_x(-controls.pitch)
                * Vec3::NEG_Z;
            assert!(look.dot((target - controls.feet - Vec3::Y * 1.65).normalize()) > 0.9999);
        }
    }

    #[test]
    fn roof_end_view_frames_the_exposed_apex_from_outside() {
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.roof.covering = crate::RoofCovering::Thatch;
        recipe.roof.rise = 4.;
        let prototype = generate(&recipe).unwrap();
        let mut controls = CameraControls::default();
        controls.frame(&prototype, "roof-end");
        assert!(!controls.walkthrough);
        let target = bevy_to_native(controls.target);
        assert!(target[1] <= -recipe.depth * 0.5 - recipe.roof.overhang + 0.00001);
        let apex = recipe.wall_height + recipe.roof.rise;
        let half_view = controls.distance * 22.5_f32.to_radians().tan();
        assert!(target[2] + half_view > apex + recipe.roof.effective_thickness() + 0.15);
        assert!(target[2] - half_view < apex - 1.1);
        assert!(controls.distance < recipe.width);
        controls.frame(&prototype, "roof-under");
        assert!(controls.pitch < 0. && !controls.walkthrough);
        controls.frame(&prototype, "eaves");
        assert!(
            (controls.target.y + controls.distance * controls.pitch.sin() - 1.65).abs() < 0.00001
        );
        assert!(controls.distance > recipe.width);
    }

    #[test]
    fn window_view_frames_the_recess_from_an_oblique_exterior_angle() {
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 128;
        let prototype = generate(&recipe).unwrap();
        let opening = recipe
            .openings
            .iter()
            .find(|o| o.kind == crate::OpeningKind::Window)
            .unwrap();
        let mut controls = CameraControls::default();
        controls.frame(&prototype, "window");
        let target = native_to_bevy(opening.facade.point(
            &recipe,
            opening.offset,
            opening.sill + opening.height * 0.5,
            recipe.wall_thickness * 0.48,
        ));
        assert!(controls.target.abs_diff_eq(target, 0.00001));
        assert!(controls.distance > opening.height * 2. && !controls.walkthrough);
        let outward = Vec3::new(
            controls.yaw.sin() * controls.pitch.cos(),
            controls.pitch.sin(),
            controls.yaw.cos() * controls.pitch.cos(),
        );
        let normal = native_to_bevy(opening.facade.normal());
        assert!(outward.dot(normal) > 0.95);
    }

    #[test]
    fn overview_frames_chimney_bounds_and_keeps_legacy_view_without_one() {
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 128;
        let prototype = generate(&recipe).unwrap();
        let mut controls = CameraControls::default();
        controls.frame(&prototype, "exterior");
        let min = native_to_bevy(prototype.layout.bounds_min);
        let max = native_to_bevy(prototype.layout.bounds_max);
        assert!(controls.target.abs_diff_eq((min + max) * 0.5, 1e-5));
        assert!(controls.distance * 22.5_f32.to_radians().sin() > min.distance(max) * 0.5);
        recipe.chimney = None;
        let legacy = generate(&recipe).unwrap();
        controls.frame(&legacy, "exterior");
        assert!(
            controls
                .target
                .abs_diff_eq(Vec3::new(0.0, recipe.wall_height * 0.5, 0.0), 1e-5)
        );
        assert!((controls.distance - recipe.width.max(recipe.depth) * 2.25).abs() < 1e-5);
    }
}

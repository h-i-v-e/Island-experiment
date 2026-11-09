//! Surface walking in normalized render space, with speeds/heights in metres.
use crate::{
    ControlsInput, Orbit, Scene,
    collision::Ground,
    command::{BedView, GenerationMode},
    orbit_transform,
};
use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

const EYE_HEIGHT: f32 = 1.7;
#[allow(clippy::cast_possible_truncation)] // Shared physical constant, converted for the f32 movement integrator.
const GRAVITY: f32 = planet_rs::solar::gravity::PLANET_SURFACE_GRAVITY as f32;

#[derive(Resource, Default)]
pub(super) struct Explorer {
    player: Option<Player>,
    pub flight: Option<crate::flight::Flight>,
    pub return_to_orbit: bool,
    press: Option<Vec2>,
    drag: f32,
    ground: Option<Ground>,
    geometry: Option<(usize, BedView, GenerationMode)>,
    lens: Option<(f32, f32)>,
}

struct Player {
    up: Vec3,
    heading: Vec3,
    feet: f32,
    vertical_speed: f32,
    pitch: f32,
    captured: bool,
}

impl Explorer {
    pub(crate) fn shared_ground(&self, scene: &Scene) -> Option<Ground> {
        (self.geometry
            == Some((
                scene.terrain.elevations.as_ptr() as usize,
                scene.bed,
                scene.generation,
            )))
        .then(|| self.ground.as_ref().map(Ground::terrain_query))
        .flatten()
    }

    pub(crate) fn adopt_ground(&mut self, scene: &Scene, ground: Ground) {
        if self.shared_ground(scene).is_none() {
            self.geometry = Some((
                scene.terrain.elevations.as_ptr() as usize,
                scene.bed,
                scene.generation,
            ));
            self.ground = Some(ground);
        }
    }

    pub(crate) fn clear_detail_rocks(&mut self) {
        if let Some(ground) = &mut self.ground {
            ground.set_rocks(Vec::new());
        }
    }
    pub(crate) fn terrain_ground(&mut self, scene: &Scene) -> &mut Ground {
        self.ensure_ground(scene);
        self.ground.as_mut().expect("displayed terrain query index")
    }
    pub fn navigating(&self) -> bool {
        self.walking() || self.flight.is_some()
    }
    pub fn walking(&self) -> bool {
        self.player.is_some()
    }
    pub(crate) fn sea_contact(&self, radius: f32) -> Option<Vec3> {
        self.player
            .as_ref()
            .filter(|p| (p.feet - 1.0).abs() * radius < 0.025 && p.vertical_speed <= 0.0)
            .map(|p| p.up)
    }
    pub fn captured(&self) -> bool {
        self.flight.as_ref().is_some_and(|flight| flight.captured)
            || self.player.as_ref().is_some_and(|player| player.captured)
    }

    fn ensure_ground(&mut self, scene: &Scene) {
        let geometry = (
            scene.terrain.elevations.as_ptr() as usize,
            scene.bed,
            scene.generation,
        );
        if self.geometry != Some(geometry) {
            let ground = Ground::new(scene);
            if let Some(player) = &mut self.player {
                player.feet = support(&ground, scene, player.up).unwrap_or(player.feet);
                player.vertical_speed = 0.0;
            }
            self.ground = Some(ground);
            self.geometry = Some(geometry);
        }
    }
    fn land(
        &mut self,
        scene: &Scene,
        ray: Ray3d,
        transform: &mut Transform,
        projection: &mut Projection,
        radius: f32,
    ) {
        self.ensure_ground(scene);
        if let Some(ground) = &self.ground {
            let mut point = ground.hit(&scene.terrain, ray.origin, *ray.direction, 50.0);
            if !scene.no_ocean
                && !scene.terrain.is_moon()
                && scene.generation == GenerationMode::Terrain
                && let Some(sea) = sea_hit(ray.origin, *ray.direction)
                && point.is_none_or(|p| {
                    sea.distance_squared(ray.origin) < p.distance_squared(ray.origin)
                })
            {
                point = Some(sea);
            }
            if let Some(point) = point
                && let Some(feet) = support(ground, scene, point.normalize())
            {
                self.player = Some(Player::land(
                    point,
                    *transform.forward(),
                    *transform.up(),
                    feet,
                ));
                if let Projection::Perspective(perspective) = projection {
                    self.lens = Some((perspective.fov, perspective.near));
                    perspective.fov = 75.0_f32.to_radians();
                    perspective.near = (0.02 / radius).clamp(1.0e-8, 0.00005);
                }
                if let Some(player) = &self.player {
                    *transform = player.transform(radius);
                }
            }
        }
    }
}

impl Player {
    fn land(point: Vec3, forward: Vec3, camera_up: Vec3, feet: f32) -> Self {
        let up = point.normalize();
        let tangent = |v: Vec3| (v - up * v.dot(up)).try_normalize();
        let heading = tangent(forward)
            .or_else(|| tangent(camera_up))
            .unwrap_or_else(|| up.any_orthonormal_vector());
        Self {
            up,
            heading,
            feet,
            vertical_speed: 0.0,
            pitch: -0.08,
            captured: true,
        }
    }

    fn transform(&self, radius: f32) -> Transform {
        Transform::from_translation(self.up * (self.feet + EYE_HEIGHT / radius)).looking_to(
            self.heading * self.pitch.cos() + self.up * self.pitch.sin(),
            self.up,
        )
    }

    #[cfg(test)]
    fn step(
        &mut self,
        ground: &Ground,
        scene: &Scene,
        input: Vec2,
        speed: f32,
        radius: f32,
        dt: f32,
    ) {
        self.step_with_buildings(ground, scene, input, speed, radius, dt, None);
    }

    #[allow(clippy::too_many_arguments)]
    fn step_with_buildings(
        &mut self,
        ground: &Ground,
        scene: &Scene,
        input: Vec2,
        speed: f32,
        radius: f32,
        dt: f32,
        buildings: Option<&crate::buildings::Contacts>,
    ) {
        let previous = support_with_buildings(ground, scene, self.up, self.feet, buildings)
            .unwrap_or(self.feet);
        let grounded = self.vertical_speed <= 0.0 && self.feet <= previous + 0.02 / radius;
        let right = self.heading.cross(self.up).normalize();
        let movement = (right * input.x + self.heading * input.y) * (speed * dt / radius);
        let candidate = (self.up * self.feet + movement).normalize();
        let Some(next_floor) =
            support_with_buildings(ground, scene, candidate, self.feet, buildings)
        else {
            return;
        };
        // Grounded slopes up to 50 degrees and steps up to 35 cm are walkable.
        // A cliff blocks horizontal movement instead of teleporting us uphill.
        let climb = (next_floor - self.feet) * radius;
        let allowed = if grounded {
            0.35 + movement.length() * radius * 50.0_f32.to_radians().tan()
        } else {
            0.02
        };
        let blocked = buildings.is_some_and(|contacts| {
            crate::buildings::blocked(
                contacts,
                candidate * next_floor.max(self.feet),
                candidate,
                radius,
            )
        });
        let floor = if climb <= allowed && !blocked {
            let turn = Quat::from_rotation_arc(self.up, candidate);
            let heading = turn * self.heading;
            self.heading = (heading - candidate * heading.dot(candidate)).normalize();
            self.up = candidate;
            next_floor
        } else {
            previous
        };
        if grounded && self.vertical_speed <= 0.0 && floor >= self.feet {
            self.feet = floor;
        }
        self.vertical_speed -= GRAVITY * dt;
        let previous_feet = self.feet;
        self.feet += self.vertical_speed * dt / radius;
        if self.vertical_speed > 0.0
            && buildings.is_some_and(|contacts| {
                crate::buildings::blocked(contacts, self.up * self.feet, self.up, radius)
            })
        {
            self.feet = previous_feet;
            self.vertical_speed = 0.0;
        }
        if self.feet <= floor {
            self.feet = floor;
            self.vertical_speed = 0.0;
        }
    }
}

#[allow(clippy::cast_possible_truncation)] // Same validated renderer radius as movement.
fn support_with_buildings(
    ground: &Ground,
    scene: &Scene,
    up: Vec3,
    feet: f32,
    buildings: Option<&crate::buildings::Contacts>,
) -> Option<f32> {
    let terrain = support(ground, scene, up);
    let radius = scene.terrain.topology.radius as f32;
    let building = buildings
        .and_then(|contacts| crate::buildings::support(contacts, up * feet, up, 0.35 / radius));
    terrain.into_iter().chain(building).max_by(f32::total_cmp)
}

fn support(ground: &Ground, scene: &Scene, up: Vec3) -> Option<f32> {
    ground.radius(&scene.terrain, up).map(|height| {
        if scene.no_ocean || scene.terrain.is_moon() || scene.generation == GenerationMode::Sphere {
            height
        } else {
            height.max(1.0)
        }
    })
}

fn sea_hit(origin: Vec3, direction: Vec3) -> Option<Vec3> {
    let b = origin.dot(direction);
    let discriminant = b * b - (origin.length_squared() - 1.0);
    if discriminant < 0.0 {
        return None;
    }
    let distance = -b - discriminant.sqrt();
    (distance >= 0.0).then_some(origin + direction * distance)
}

fn movement_intent(keys: &ButtonInput<KeyCode>) -> (Vec2, f32) {
    let axis = |positive, negative| {
        f32::from(u8::from(keys.pressed(positive))) - f32::from(u8::from(keys.pressed(negative)))
    };
    let movement = Vec2::new(
        axis(KeyCode::KeyD, KeyCode::KeyA),
        axis(KeyCode::KeyW, KeyCode::KeyS),
    )
    .clamp_length_max(1.0);
    let running = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    (movement, if running { 9.0 } else { 4.5 })
}

fn return_to_orbit(
    explorer: &mut Explorer,
    transform: &mut Transform,
    projection: &mut Projection,
    orbit: &Orbit,
) {
    explorer.player = None;
    explorer.flight = None;
    explorer.return_to_orbit = false;
    explorer.press = None;
    *transform = orbit_transform(orbit);
    if let Projection::Perspective(perspective) = projection
        && let Some((fov, near)) = explorer.lens.take()
    {
        perspective.fov = fov;
        perspective.near = near;
    }
}

fn flight_controls(
    explorer: &mut Explorer,
    input: &ControlsInput,
    solar_state: Option<planet_rs::solar::SolarState>,
    camera: (&Orbit, &mut Transform, &mut Projection),
    window: (&Window, &mut CursorOptions),
    seconds: f64,
    radius: f32,
) -> bool {
    let (orbit, transform, projection) = camera;
    let (window, cursor) = window;
    let keyboard = !input.wants.wants_any_keyboard_input();
    let pointer = !input.wants.wants_any_pointer_input();
    if keyboard
        && input.keys.just_pressed(KeyCode::KeyF)
        && explorer.flight.is_none()
        && let Some(state) = solar_state
    {
        explorer.flight = Some(crate::flight::Flight::enter(
            transform,
            state,
            f64::from(radius),
        ));
        explorer.player = None;
        if let Projection::Perspective(lens) = &mut *projection {
            if explorer.lens.is_none() {
                explorer.lens = Some((lens.fov, lens.near));
            }
            lens.fov = 75.0_f32.to_radians();
        }
    } else if explorer.flight.is_some() && input.keys.just_pressed(KeyCode::KeyF) {
        explorer.return_to_orbit = true;
    }
    if explorer.flight.is_some() {
        if explorer.return_to_orbit
            || input.keys.just_pressed(KeyCode::Escape)
            || solar_state.is_none()
        {
            return_to_orbit(explorer, transform, projection, orbit);
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        } else if let (Some(flight), Some(state)) = (&mut explorer.flight, solar_state) {
            if !window.focused || input.keys.just_pressed(KeyCode::Tab) {
                flight.captured = false;
            } else if pointer && input.buttons.just_pressed(MouseButton::Left) {
                flight.captured = true;
            }
            if flight.captured && window.focused {
                let axis = |a, b| {
                    f32::from(u8::from(input.keys.pressed(a)))
                        - f32::from(u8::from(input.keys.pressed(b)))
                };
                let movement = if keyboard {
                    Vec3::new(
                        axis(KeyCode::KeyD, KeyCode::KeyA),
                        axis(KeyCode::KeyE, KeyCode::KeyQ),
                        axis(KeyCode::KeyS, KeyCode::KeyW),
                    )
                } else {
                    Vec3::ZERO
                };
                let scroll = input.scroll.delta.y
                    * if input.scroll.unit == crate::MouseScrollUnit::Line {
                        1.0
                    } else {
                        0.05
                    };
                flight.advance(
                    movement,
                    input.motion.delta,
                    scroll,
                    input.keys.pressed(KeyCode::ShiftLeft)
                        || input.keys.pressed(KeyCode::ShiftRight),
                    seconds,
                    state.sun_radius,
                );
            }
            *transform = flight.camera(state, f64::from(radius));
            cursor.grab_mode = if flight.captured && window.focused {
                CursorGrabMode::Locked
            } else {
                CursorGrabMode::None
            };
            cursor.visible = !flight.captured || !window.focused;
        }
        return true;
    }
    false
}

#[allow(clippy::cast_possible_truncation)] // Validated physical radius converted to renderer units.
fn walking_controls(
    explorer: &mut Explorer,
    input: &ControlsInput,
    scene: &Scene,
    window: &Window,
    camera: (&Orbit, &mut Transform, &mut Projection),
    time: &Time,
    buildings: &crate::buildings::Contacts,
) {
    let (orbit, transform, projection) = camera;
    let keyboard = !input.wants.wants_any_keyboard_input();
    let pointer = !input.wants.wants_any_pointer_input();
    let radius = scene.terrain.topology.radius as f32;
    // These remain available when Tab gives the HUD button keyboard focus.
    if explorer.return_to_orbit || input.keys.just_pressed(KeyCode::Escape) {
        return_to_orbit(explorer, transform, projection, orbit);
    } else {
        explorer.ensure_ground(scene);
        let Explorer { player, ground, .. } = explorer;
        if let (Some(player), Some(ground)) = (player, ground) {
            let was_captured = player.captured;
            if !window.focused || input.keys.just_pressed(KeyCode::Tab) {
                player.captured = false;
            } else if pointer && input.buttons.just_pressed(MouseButton::Left) {
                player.captured = true;
            }
            if player.captured && window.focused {
                if was_captured {
                    player.heading =
                        (Quat::from_axis_angle(player.up, -input.motion.delta.x * 0.0025)
                            * player.heading)
                            .normalize();
                    player.pitch = (player.pitch - input.motion.delta.y * 0.0025).clamp(-1.5, 1.5);
                }
                if keyboard {
                    let (movement, speed) = movement_intent(&input.keys);
                    if input.keys.just_pressed(KeyCode::Space)
                        && support_with_buildings(
                            ground,
                            scene,
                            player.up,
                            player.feet,
                            Some(buildings),
                        )
                        .is_some_and(|floor| player.feet <= floor + 0.02 / radius)
                    {
                        player.vertical_speed = 5.0;
                    }
                    let mut remaining = time.delta_secs().min(0.1);
                    while remaining > 0.0 {
                        let dt = remaining.min(1.0 / 120.0);
                        player.step_with_buildings(
                            ground,
                            scene,
                            movement,
                            speed,
                            radius,
                            dt,
                            Some(buildings),
                        );
                        remaining -= dt;
                    }
                }
            }
            *transform = player.transform(radius);
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)] // Bevy camera/input/collision parameters.
pub(super) fn update(
    input: ControlsInput,
    time: Res<Time>,
    scene: Res<Scene>,
    mut explorer: ResMut<Explorer>,
    solar: Option<Res<crate::solar::SolarFrame>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut cameras: Query<(&Camera, &Orbit, &mut Transform, &mut Projection)>,
    buildings: crate::buildings::Contacts,
) {
    let Ok((window, mut cursor)) = windows.single_mut() else {
        return;
    };
    let Ok((camera, orbit, mut transform, mut projection)) = cameras.single_mut() else {
        return;
    };
    let pointer = !input.wants.wants_any_pointer_input();
    let radius = scene.terrain.topology.radius as f32;
    if flight_controls(
        &mut explorer,
        &input,
        solar.as_ref().and_then(|s| s.state),
        (orbit, &mut transform, &mut projection),
        (window, &mut cursor),
        time.delta_secs_f64(),
        radius,
    ) {
        return;
    }
    if explorer.walking() {
        walking_controls(
            &mut explorer,
            &input,
            &scene,
            window,
            (orbit, &mut transform, &mut projection),
            &time,
            &buildings,
        );
    } else {
        if pointer && input.buttons.just_pressed(MouseButton::Left) {
            explorer.press = window.cursor_position().filter(|&p| {
                camera
                    .logical_viewport_rect()
                    .is_some_and(|rect| rect.contains(p))
            });
            explorer.drag = 0.0;
        }
        if input.buttons.pressed(MouseButton::Left)
            && !input.buttons.just_pressed(MouseButton::Left)
        {
            explorer.drag += input.motion.delta.length();
        }
        if input.buttons.just_released(MouseButton::Left) {
            let press = explorer.press.take();
            if pointer
                && explorer.drag < 5.0
                && let Some(position) = press
                && window
                    .cursor_position()
                    .is_some_and(|p| p.distance(position) < 5.0)
                && let Ok(ray) =
                    camera.viewport_to_world(&GlobalTransform::from(*transform), position)
            {
                explorer.land(&scene, ray, &mut transform, &mut projection, radius);
            }
        }
    }
    let captured = explorer.captured() && window.focused;
    cursor.visible = !captured;
    cursor.grab_mode = if captured {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{collision::test_scene, command::View, initial_orbit};

    #[test]
    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)] // One complete closed/open/jump scenario at several radial sites.
    fn building_collision_blocks_closed_doors_and_allows_open_door_entry() {
        use bevy::ecs::system::SystemState;
        use island_building::{
            BuildingRecipe, generate,
            interaction::BuildingInstance,
            render::{bevy_to_native, compile_building},
        };
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 128;
        let compiled = compile_building(
            std::sync::Arc::new(generate(&recipe).unwrap()),
            &mut Assets::default(),
            &mut Assets::default(),
            &mut Assets::default(),
        );
        for up in [Vec3::Y, Vec3::NEG_Y, Vec3::X] {
            let scene = crate::buildings::test_flat_site(up);
            let ground = Ground::new(&scene);
            let radius = scene.terrain.topology.radius as f32;
            let ground_height = ground.radius(&scene.terrain, up).unwrap();
            let root = Transform::from_translation(up * (ground_height + 0.1 / radius))
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, up))
                .with_scale(Vec3::splat(radius.recip()));
            let global = GlobalTransform::from(root);
            let outside = root.transform_point(Vec3::new(0.0, 0.005, recipe.depth * 0.5 + 1.0));
            let heading = root.rotation * Vec3::NEG_Z;
            let mut player = Player::land(outside, heading, up, outside.length());
            let mut world = World::new();
            let entity = world
                .spawn((
                    BuildingInstance::new(
                        std::sync::Arc::clone(&compiled),
                        String::from("entry-test"),
                    ),
                    global,
                ))
                .id();
            let mut state = SystemState::<crate::buildings::Contacts>::new(&mut world);
            {
                let contacts = state.get(&world).expect("valid building contact query");
                for _ in 0..100 {
                    player.step_with_buildings(
                        &ground,
                        &scene,
                        Vec2::Y,
                        2.8,
                        radius,
                        1.0 / 120.0,
                        Some(&contacts),
                    );
                }
            }
            let closed = bevy_to_native(
                global
                    .affine()
                    .inverse()
                    .transform_point3(player.up * player.feet),
            );
            assert!(
                closed[1] < -recipe.depth * 0.5,
                "closed leaf must block the normal controller"
            );
            {
                let mut instance = world.get_mut::<BuildingInstance>(entity).unwrap();
                let angle = instance.prototype.source.doors[0].swing_angle;
                instance.doors.get_mut("entrance").unwrap().angle = angle;
            }
            {
                let contacts = state.get(&world).expect("valid building contact query");
                for _ in 0..125 {
                    player.step_with_buildings(
                        &ground,
                        &scene,
                        Vec2::Y,
                        2.8,
                        radius,
                        1.0 / 120.0,
                        Some(&contacts),
                    );
                }
            }
            let opened = bevy_to_native(
                global
                    .affine()
                    .inverse()
                    .transform_point3(player.up * player.feet),
            );
            assert!(
                opened[1] > -recipe.depth * 0.5 + 0.5,
                "open door must allow entry: {opened:?}"
            );
            assert!(
                opened[2].abs() < 0.01,
                "interior floor must support radial walking: {opened:?}"
            );
            {
                let contacts = state.get(&world).expect("valid building contact query");
                player.vertical_speed = 5.0;
                let mut peak: f32 = 0.0;
                for _ in 0..240 {
                    player.step_with_buildings(
                        &ground,
                        &scene,
                        Vec2::ZERO,
                        2.8,
                        radius,
                        1.0 / 120.0,
                        Some(&contacts),
                    );
                    let position = bevy_to_native(
                        global
                            .affine()
                            .inverse()
                            .transform_point3(player.up * player.feet),
                    );
                    peak = peak.max(position[2]);
                }
                assert!(
                    peak > 0.6 && peak < recipe.wall_height - 1.7,
                    "ceiling must stop the capsule during a normal jump: {peak}"
                );
            }
        }
    }

    #[test]
    fn flight_entry_movement_mouse_release_and_return_use_the_normal_controller_path() {
        use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
        use bevy::time::TimeUpdateStrategy;
        use bevy_egui::input::EguiWantsInput;
        use std::time::Duration;
        let scene = test_scene();
        let state = planet_rs::solar::SolarSystem::new(
            planet_rs::solar::SolarSettings::default(),
            scene.terrain.topology.radius,
        )
        .unwrap()
        .state(0.0);
        let orbit = initial_orbit(View::Front, &scene.terrain);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<EguiWantsInput>()
            .init_resource::<Explorer>()
            .init_resource::<crate::solar::SolarFrame>()
            .insert_resource(crate::InitialOrbit(orbit))
            .insert_resource(scene)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                100,
            )))
            .add_systems(Update, update);
        app.world_mut()
            .resource_mut::<crate::solar::SolarFrame>()
            .state = Some(state);
        app.world_mut().spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut().spawn((
            Camera::default(),
            orbit,
            crate::orbit_transform(&orbit),
            Projection::default(),
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyF);
        app.update();
        assert!(app.world().resource::<Explorer>().flight.is_some());
        let start = app
            .world()
            .resource::<Explorer>()
            .flight
            .as_ref()
            .unwrap()
            .position;
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            keys.press(KeyCode::KeyW);
        }
        app.update();
        let moved = app
            .world()
            .resource::<Explorer>()
            .flight
            .as_ref()
            .unwrap()
            .position;
        assert!(moved.distance(start) > 900.0);
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            keys.press(KeyCode::Tab);
            keys.press(KeyCode::KeyW);
        }
        app.update();
        let explorer = app.world().resource::<Explorer>();
        assert!(!explorer.captured());
        assert_eq!(explorer.flight.as_ref().unwrap().position, moved);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert!(app.world().resource::<Explorer>().captured());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<Explorer>().navigating());
    }

    #[test]
    fn mouse_look_and_navigation_keys_work_even_when_the_hud_has_keyboard_focus() {
        use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
        use bevy_egui::{
            EguiContext,
            input::{EguiWantsInput, write_egui_wants_input_system},
        };
        let scene = test_scene();
        let orbit = initial_orbit(View::Front, &scene.terrain);
        let mut explorer = Explorer::default();
        explorer.ensure_ground(&scene);
        explorer.player = Some(Player::land(Vec3::Y, Vec3::X, Vec3::Z, 1.0));
        let mut context = EguiContext::default();
        let mut output = context
            .get_mut()
            .run_ui(bevy_egui::egui::RawInput::default(), |ui| {
                ui.button("Return to orbit").request_focus();
            });
        output.textures_delta.clear();
        assert!(context.get_mut().egui_wants_keyboard_input());
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<EguiWantsInput>()
            .insert_resource(AccumulatedMouseMotion {
                delta: Vec2::new(100.0, 20.0),
            })
            .insert_resource(crate::InitialOrbit(orbit))
            .insert_resource(explorer)
            .insert_resource(scene)
            .add_systems(Update, (write_egui_wants_input_system, update).chain());
        app.world_mut().spawn(context);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    focused: true,
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                Camera::default(),
                orbit,
                Transform::default(),
                Projection::default(),
            ))
            .id();
        app.update();
        let player = app.world().resource::<Explorer>().player.as_ref().unwrap();
        assert!(
            player.heading.distance(Vec3::X) > 0.1,
            "mouse delta must turn the view"
        );
        assert!(player.pitch < -0.1);
        assert!(
            app.world()
                .resource::<EguiWantsInput>()
                .wants_any_keyboard_input()
        );
        let before_release = *app.world().get::<Transform>(camera).unwrap();
        for _ in 0..2 {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            keys.press(KeyCode::Tab);
            app.update();
            assert!(!app.world().resource::<Explorer>().captured());
            let cursor = app.world().get::<CursorOptions>(window).unwrap();
            assert!(cursor.visible);
            assert_eq!(cursor.grab_mode, CursorGrabMode::None);
            assert_eq!(
                *app.world().get::<Transform>(camera).unwrap(),
                before_release,
                "released mouse motion must leave the screenshot view unchanged"
            );
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert!(app.world().resource::<Explorer>().captured());
        let cursor = app.world().get::<CursorOptions>(window).unwrap();
        assert!(!cursor.visible);
        assert_eq!(cursor.grab_mode, CursorGrabMode::Locked);
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<Explorer>().walking());
        assert!(app.world().get::<CursorOptions>(window).unwrap().visible);
    }

    #[test]
    fn walking_at_both_poles_keeps_radial_up_ground_contact_and_metre_scale() {
        let scene = test_scene();
        let ground = Ground::new(&scene);
        let radius = 2000.0_f32;
        assert!((scene.terrain.topology.radius - f64::from(radius)).abs() < f64::EPSILON);
        for up in [Vec3::Y, -Vec3::Y, Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z] {
            let feet = support(&ground, &scene, up).unwrap();
            let mut player = Player::land(up * feet, -up, Vec3::Y, feet);
            for _ in 0..120 {
                player.step(&ground, &scene, Vec2::Y, 4.5, radius, 1.0 / 120.0);
            }
            assert!((player.up.distance(up) * radius - 4.5).abs() < 0.05);
            assert!(player.up.dot(player.heading).abs() < 1.0e-5);
            assert!(player.feet >= support(&ground, &scene, player.up).unwrap() - 1.0e-6);
            let camera = player.transform(radius);
            assert!(
                (camera.translation.length() - player.feet - EYE_HEIGHT / radius).abs() < 1.0e-6
            );
            assert!(camera.up().dot(player.up) > 0.99);
        }
    }

    #[test]
    fn moon_basin_floor_has_no_invisible_sea_plane() {
        let mut scene = test_scene();
        let terrain = planet_rs::generate_terrain(
            planet_rs::PlanetOptions {
                vertex_count: 256,
                radius: 350.0,
                ..default()
            },
            planet_rs::TerrainOptions {
                source: planet_rs::TerrainSource::Moon,
                moon: planet_rs::terrain::moon::MoonOptions {
                    refinements: 1,
                    ..default()
                },
                ..default()
            },
        )
        .unwrap();
        let low = terrain
            .elevations
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        let up = crate::surface::vector(terrain.topology.directions[low]);
        scene.terrain = std::sync::Arc::new(terrain);
        let ground = Ground::new(&scene);
        let floor = support(&ground, &scene, up).unwrap();
        assert!(
            floor < 0.99,
            "a crater basin must remain below the reference sphere"
        );
        assert!((floor - ground.radius(&scene.terrain, up).unwrap()).abs() < 1e-6);
    }

    #[test]
    fn radial_gravity_returns_jumps_to_ground_and_visible_seas_support_the_player() {
        let mut scene = test_scene();
        let ground = Ground::new(&scene);
        let radius = 2000.0_f32;
        assert!((scene.terrain.topology.radius - f64::from(radius)).abs() < f64::EPSILON);
        for up in [Vec3::Y, -Vec3::Y] {
            let feet = support(&ground, &scene, up).unwrap();
            let mut player = Player::land(up * feet, Vec3::Z, Vec3::Y, feet);
            player.vertical_speed = 5.0;
            let mut peak = feet;
            for _ in 0..240 {
                player.step(&ground, &scene, Vec2::ZERO, 4.5, radius, 1.0 / 120.0);
                peak = peak.max(player.feet);
            }
            assert!((peak - feet) * radius > 1.2);
            assert!((player.feet - feet).abs() < 1.0e-6);
            assert!(player.vertical_speed.abs() < f32::EPSILON);
        }
        scene.no_ocean = false;
        assert!((support(&ground, &scene, Vec3::Y).unwrap() - 1.0).abs() < f32::EPSILON);
        assert!(sea_hit(Vec3::Z * 3.0, -Vec3::Z).unwrap().distance(Vec3::Z) < 1.0e-6);
    }

    #[test]
    fn leaving_first_person_restores_orbit_and_lens_and_new_geometry_rebuilds_collision() {
        let mut scene = test_scene();
        let orbit = initial_orbit(View::Back, &scene.terrain);
        let mut explorer = Explorer::default();
        explorer.ensure_ground(&scene);
        let player = Player::land(Vec3::Y, Vec3::X, Vec3::Z, 1.0);
        let mut transform = player.transform(1000.0);
        explorer.player = Some(player);
        explorer.lens = Some((0.7, 0.00005));
        let mut projection = Projection::Perspective(PerspectiveProjection::default());
        return_to_orbit(&mut explorer, &mut transform, &mut projection, &orbit);
        assert!(!explorer.walking());
        assert_eq!(transform, orbit_transform(&orbit));
        let Projection::Perspective(lens) = projection else {
            panic!("perspective lens")
        };
        assert!((lens.fov - 0.7).abs() < f32::EPSILON);
        assert!((lens.near - 0.00005).abs() < f32::EPSILON);
        scene.bed = BedView::Original;
        explorer.ensure_ground(&scene);
        assert_eq!(explorer.geometry.unwrap().1, BedView::Original);
        let first = explorer.geometry;
        scene.terrain = test_scene().terrain;
        explorer.ensure_ground(&scene);
        assert_ne!(explorer.geometry, first);
    }
}

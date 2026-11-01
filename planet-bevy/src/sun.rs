//! The star is a persistent mesh at the heliocentric origin. Planet-local GPU
//! coordinates are just a render transform, not a separate astronomical model.
use crate::{
    Orbit, Scene,
    command::View,
    solar::{SolarFrame, SolarUpdate},
};
use bevy::{
    asset::embedded_asset,
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
use planet_rs::solar::SolarState;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SunMaterial {
    #[uniform(0)]
    pub phase: Vec4,
}
impl Material for SunMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://planet_bevy/shaders/sun.wgsl".into()
    }
}
#[derive(Component)]
struct SunBody;
#[derive(Resource, Default)]
pub struct SunWorld {
    entity: Option<Entity>,
    pub position: Vec3,
    pub radius: f32,
    pub active: bool,
    inspecting: Option<View>,
}
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct SunBodyUpdate;
pub struct SunBodyPlugin;
impl Plugin for SunBodyPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/sun.wgsl");
        app.add_plugins(MaterialPlugin::<SunMaterial>::default())
            .init_resource::<SunWorld>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                update
                    .in_set(SunBodyUpdate)
                    .after(SolarUpdate)
                    .after(crate::explore::update)
                    .after(crate::controls),
            )
            .add_systems(Update, draw_orbit.after(SunBodyUpdate));
    }
}
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SunMaterial>>,
    mut world: ResMut<SunWorld>,
) {
    world.entity = Some(
        commands
            .spawn((
                SunBody,
                Mesh3d(
                    meshes.add(
                        Sphere::new(1.0)
                            .mesh()
                            .ico(64)
                            .expect("fixed valid solar sphere"),
                    ),
                ),
                MeshMaterial3d(materials.add(SunMaterial { phase: Vec4::ZERO })),
                Transform::default(),
                Visibility::Hidden,
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id(),
    );
}
#[allow(clippy::cast_possible_truncation)] // Single render boundary; astronomical state remains f64 metres.
fn body_transform(state: SolarState, radius: f64) -> Transform {
    Transform {
        translation: crate::surface::vector(state.sun_position() / radius),
        rotation: Quat::from_array(state.local_to_inertial.conjugate().as_quat().to_array()),
        scale: Vec3::splat((state.sun_radius / radius) as f32),
    }
}
pub fn initial_inspection(view: View, radius: f64) -> Option<Orbit> {
    matches!(view, View::Sun | View::Solar).then(|| {
        inspection_orbit(
            view,
            planet_rs::solar::SolarSystem::new(planet_rs::solar::SolarSettings::default(), radius)
                .expect("validated planet radius")
                .state(0.0),
            radius,
        )
    })
}
#[allow(clippy::cast_possible_truncation)]
pub fn inspection_orbit(view: View, state: SolarState, radius: f64) -> Orbit {
    let body = body_transform(state, radius);
    let sun = view == View::Sun;
    Orbit {
        yaw: 0.45,
        pitch: 0.35,
        distance: if sun {
            body.scale.x * 3.0
        } else {
            (body.translation.length() + body.scale.x) * 2.8
        },
        focus: body.translation,
        frame: body.rotation,
        minimum: if sun {
            body.scale.x * 1.0005
        } else {
            body.scale.x * 1.2
        },
        maximum: (body.translation.length() + body.scale.x) * 10.0,
    }
}
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::cast_possible_truncation
)]
fn update(
    frame: Res<SolarFrame>,
    scene: Res<Scene>,
    panel: Res<crate::panel::Panel>,
    explorer: Res<crate::explore::Explorer>,
    mut world: ResMut<SunWorld>,
    mut materials: ResMut<Assets<SunMaterial>>,
    mut bodies: Query<
        (
            &mut Transform,
            &mut Visibility,
            &MeshMaterial3d<SunMaterial>,
        ),
        With<SunBody>,
    >,
    mut cameras: Query<(&mut Orbit, &mut Transform, &mut Projection), Without<SunBody>>,
) {
    world.active = frame.state.is_some();
    let Some(id) = world.entity else {
        return;
    };
    let Ok((mut body, mut visibility, material)) = bodies.get_mut(id) else {
        return;
    };
    let Some(state) = frame.state else {
        *visibility = Visibility::Hidden;
        return;
    };
    *body = body_transform(state, scene.terrain.topology.radius);
    *visibility = Visibility::Visible;
    world.position = body.translation;
    world.radius = body.scale.x;
    if let Some(mut material) = materials.get_mut(&material.0) {
        material.phase.x = (frame.orbital_seconds % 10000.0) as f32;
    }
    let view = panel.inspection_view();
    let inspecting = matches!(view, View::Sun | View::Solar) && !explorer.navigating();
    for (mut orbit, mut camera, mut projection) in &mut cameras {
        if inspecting {
            let next = inspection_orbit(view, state, scene.terrain.topology.radius);
            if world.inspecting != Some(view) {
                *orbit = next;
            }
            orbit.focus = next.focus;
            orbit.frame = next.frame;
            orbit.minimum = next.minimum;
            orbit.maximum = next.maximum;
            *camera = crate::orbit_transform(&orbit);
        }
        if let Projection::Perspective(lens) = &mut *projection {
            // Reverse-Z retains surface depth precision with a distant star.
            lens.far = (camera.translation.distance(world.position) + world.radius)
                .max(camera.translation.length() + 8.0)
                .max(50.0)
                * 1.05;
        }
    }
    world.inspecting = inspecting.then_some(view);
}

fn draw_orbit(
    panel: Res<crate::panel::Panel>,
    settings: Res<crate::solar::SolarControls>,
    frame: Res<SolarFrame>,
    scene: Res<Scene>,
    mut gizmos: Gizmos,
) {
    if panel.inspection_view() != View::Solar {
        return;
    }
    let Some(state) = frame.state else {
        return;
    };
    let radius = scene.terrain.topology.radius;
    let Ok(system) = planet_rs::solar::SolarSystem::new(settings.model, radius) else {
        return;
    };
    let period = settings.model.orbital_period(radius);
    let mut previous =
        crate::surface::vector(state.inertial_to_local(system.state(0.0).planet_position) / radius);
    for i in 1..=128 {
        let next = crate::surface::vector(
            state.inertial_to_local(system.state(period * f64::from(i) / 128.0).planet_position)
                / radius,
        );
        gizmos.line(previous, next, Color::srgb(0.15, 0.32, 0.65));
        previous = next;
    }
    // A navigation marker makes the true-scale, often subpixel planet locatable.
    #[allow(clippy::cast_possible_truncation)]
    let marker = (settings.model.orbit_radius_ratio * 0.008) as f32;
    gizmos.sphere(Isometry3d::IDENTITY, marker, Color::srgb(0.2, 0.7, 1.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use planet_rs::solar::{SolarSettings, SolarSystem};
    #[test]
    fn solar_body_and_close_camera_follow_the_same_origin_and_reuse_gpu_assets() {
        let scene = crate::collision::test_scene();
        let radius = scene.terrain.topology.radius;
        let system = SolarSystem::new(SolarSettings::default(), radius).unwrap();
        let mut panel = crate::panel::Panel::new(&crate::panel::Settings::default(), false);
        panel.restore_draft(&crate::panel::Settings::default(), View::Sun);
        let state = system.state(0.0);
        let orbit = inspection_orbit(View::Sun, state, radius);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(scene)
            .insert_resource(panel)
            .init_resource::<SolarFrame>()
            .init_resource::<crate::explore::Explorer>()
            .init_resource::<SunWorld>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<SunMaterial>>()
            .add_systems(Startup, setup)
            .add_systems(Update, update);
        app.world_mut().resource_mut::<SolarFrame>().state = Some(state);
        let camera = app
            .world_mut()
            .spawn((orbit, crate::orbit_transform(&orbit), Projection::default()))
            .id();
        app.update();
        let entity = app.world().resource::<SunWorld>().entity.unwrap();
        for t in [0.0, 1000.0, 5000.0] {
            app.world_mut().resource_mut::<SolarFrame>().state = Some(system.state_at(t, t * 48.0));
            app.update();
            let world = app.world().resource::<SunWorld>();
            assert_eq!(world.entity, Some(entity));
            let body = app.world().get::<Transform>(entity).unwrap();
            assert!(
                (f64::from(body.scale.x) - SolarSettings::default().sun_radius_ratio).abs() < 1e-4
            );
            let camera = app.world().get::<Transform>(camera).unwrap();
            assert!(
                (camera.translation.distance(body.translation) / body.scale.x - 3.0).abs() < 1e-4
            );
        }
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 1);
        assert_eq!(app.world().resource::<Assets<SunMaterial>>().len(), 1);
        let Projection::Perspective(lens) = app.world().get::<Projection>(camera).unwrap() else {
            panic!("perspective camera")
        };
        assert!(lens.far > 1000.0);
        app.world_mut().resource_mut::<SolarFrame>().state = None;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
    }
}

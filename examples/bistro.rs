use std::f32::consts::PI;

use argh::FromArgs;
use bevy::{
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    core_pipeline::prepass::DepthPrepass,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    light::{
        CascadeShadowConfigBuilder, TransmittedShadowReceiver,
        light_consts::lux::{self},
    },
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    scene::SceneInstanceReady,
    window::{PresentMode, WindowMode},
    winit::WinitSettings,
};
use bevy_mod_mipmap_generator::{MipmapGeneratorPlugin, generate_mipmaps};
use bgl2::{
    bevy_standard_lighting::OpenGLStandardLightingPlugin,
    bevy_standard_material::{OpenGLStandardMaterialPlugin, OpenGLStandardMaterialSettings},
    phase_shadow::ShadowBounds,
    render::OpenGLRenderPlugins,
};
use wgpu_types::Face;

#[derive(FromArgs, Resource, Clone, Default)]
/// Config
pub struct Args {
    /// use default bevy render backend (Also need to enable default plugins)
    #[argh(switch)]
    bevy: bool,
    /// the windows xp driver often doesn't like point lights (for loop code gen too long, sometimes other things)
    #[argh(switch)]
    no_point: bool,
}

fn main() {
    #[cfg(target_arch = "wasm32")]
    let args: Args = Default::default();
    #[cfg(not(target_arch = "wasm32"))]
    let args: Args = argh::from_env();

    let mut app = App::new();
    app.insert_resource(OpenGLStandardMaterialSettings {
        no_point: args.no_point,
    })
    .insert_resource(args.clone())
    .insert_resource(ClearColor(Color::srgb(1.75 * 0.5, 1.9 * 0.5, 1.99 * 0.5)))
    .insert_resource(WinitSettings::continuous())
    .insert_resource(GlobalAmbientLight::NONE)
    .add_plugins((
        DefaultPlugins
            .set(RenderPlugin {
                render_creation: WgpuSettings {
                    backends: if args.bevy {
                        Some(wgpu_types::Backends::all())
                    } else {
                        None
                    },
                    ..default()
                }
                .into(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    present_mode: PresentMode::Immediate,
                    ..default()
                }),
                ..default()
            }),
        FreeCameraPlugin,
        LogDiagnosticsPlugin::default(),
        FrameTimeDiagnosticsPlugin::default(),
        MipmapGeneratorPlugin,
    ));

    if !args.bevy {
        app.add_plugins((
            OpenGLRenderPlugins,
            OpenGLStandardLightingPlugin,
            OpenGLStandardMaterialPlugin,
        ));
    }

    app.add_systems(Startup, setup)
        .add_systems(Update, input)
        .add_systems(Update, generate_mipmaps::<StandardMaterial>)
        .run();
}

fn input(keyboard_input: Res<ButtonInput<KeyCode>>, mut window: Single<&mut Window>) {
    if keyboard_input.just_pressed(KeyCode::F11) || keyboard_input.just_pressed(KeyCode::KeyF) {
        if window.mode == WindowMode::Windowed {
            window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Current);
        } else {
            window.mode = WindowMode::Windowed;
        }
    }
    if keyboard_input.just_pressed(KeyCode::Escape) {
        window.mode = WindowMode::Windowed;
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let bistro_exterior =
        asset_server.load("models/bistro/bistro_exterior/BistroExterior.gltf#Scene0");
    commands
        .spawn(SceneRoot(bistro_exterior.clone()))
        .observe(proc_scene);

    let bistro_interior =
        asset_server.load("models/bistro/bistro_interior_wine/BistroInterior_Wine.gltf#Scene0");
    commands
        .spawn(SceneRoot(bistro_interior.clone()))
        .observe(proc_scene);

    commands.spawn(SceneRoot(
        asset_server.load("models/bistro/BistroExteriorFakeGI.gltf#Scene0"),
    ));

    // Sun
    commands.spawn((
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, PI * -0.35, PI * -0.13, 0.0)),
        DirectionalLight {
            color: Color::srgb(1.0, 0.87, 0.78),
            illuminance: lux::FULL_DAYLIGHT,
            shadows_enabled: true,
            shadow_depth_bias: 0.1,
            shadow_normal_bias: 0.2,
            ..default()
        },
        ShadowBounds::cube(70.0),
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 0.05,
            maximum_distance: 70.0,
            first_cascade_far_bound: 10.0,
            overlap_proportion: 0.2,
        }
        .build(),
    ));

    // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-10.5, 1.7, -1.0).looking_at(Vec3::new(0.0, 3.5, 0.0), Vec3::Y),
        Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::PI / 3.0,
            near: 0.1,
            far: 1000.0,
            aspect_ratio: 1.0,
            ..Default::default()
        }),
        EnvironmentMapLight {
            diffuse_map: asset_server
                .load("models/bistro/bistro_env_map/san_giuseppe_bridge_4k_diffuse.ktx2"),
            specular_map: asset_server
                .load("models/bistro/bistro_env_map/san_giuseppe_bridge_4k_specular.ktx2"),
            intensity: 600.0,
            ..default()
        },
        FreeCamera::default(),
        DepthPrepass,
    ));
}

#[allow(clippy::type_complexity)]
pub fn proc_scene(
    scene_ready: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    has_std_mat: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lights: Query<Entity, Or<(With<PointLight>, With<DirectionalLight>, With<SpotLight>)>>,
    cameras: Query<Entity, With<Camera>>,
) {
    for entity in children.iter_descendants(scene_ready.entity) {
        if let Ok(mat_h) = has_std_mat.get(entity) {
            if let Some(mat) = materials.get_mut(mat_h) {
                mat.flip_normal_map_y = true;
                match mat.alpha_mode {
                    AlphaMode::Mask(_) => {
                        mat.diffuse_transmission = 0.6;
                        mat.double_sided = true;
                        mat.cull_mode = None;
                        commands.entity(entity).insert(TransmittedShadowReceiver);
                    }
                    _ => {
                        mat.double_sided = false;
                        mat.cull_mode = Some(Face::Back);
                    }
                }
            }
        }

        // Remove any cameras in the gltf scene
        if lights.get(entity).is_ok() || cameras.get(entity).is_ok() {
            commands.entity(entity).despawn();
        }
    }
}

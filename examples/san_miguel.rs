use std::f32::consts::PI;

use argh::FromArgs;
use bevy::{
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    core_pipeline::{prepass::DepthPrepass, tonemapping::Tonemapping},
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    light::{TransmittedShadowReceiver, light_consts::lux::DIRECT_SUNLIGHT},
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
    // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-10.5, 1.7, -1.0).looking_at(Vec3::new(0.0, 2.5, 0.0), Vec3::Y),
        Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::PI / 2.8,
            ..default()
        }),
        EnvironmentMapLight {
            diffuse_map: asset_server.load("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
            specular_map: asset_server.load("environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
            intensity: 500.0,
            ..default()
        },
        DepthPrepass,
        FreeCamera::default(),
        Tonemapping::TonyMcMapface,
    ));

    commands
        .spawn((
            SceneRoot(asset_server.load("models/san-miguel/san-miguel.gltf#Scene0")),
            Transform::from_xyz(-18.0, 0.0, 0.0),
        ))
        .observe(proc_scene);

    //commands
    //    .spawn((
    //        SceneRoot(
    //            asset_server.load("models/bistro/bistro_exterior/BistroExterior.gltf#Scene0"),
    //        ),
    //        Transform::from_xyz(0.0, 0.0, 0.0),
    //    ))
    //    .observe(proc_scene);
    //commands
    //    .spawn((
    //        SceneRoot(
    //            asset_server
    //                .load("models/bistro/bistro_interior_wine/BistroInterior_Wine.gltf#Scene0"),
    //        ),
    //        Transform::from_xyz(0.0, 0.0, 0.0),
    //    ))
    //    .observe(proc_scene);

    //commands
    //    .spawn((
    //        SceneRoot(asset_server.load("models/caldera/hotel_01.glb#Scene0")),
    //        Transform::from_scale(Vec3::ONE * 0.01),
    //    ))
    //    .observe(proc_scene);

    // Sun
    commands.spawn((
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, PI * -0.43, PI * -0.08, 0.0)),
        DirectionalLight {
            color: Color::srgb(1.0, 0.9, 0.8),
            illuminance: DIRECT_SUNLIGHT,
            shadows_enabled: true,
            shadow_depth_bias: 0.3,
            shadow_normal_bias: 0.6,
            ..default()
        },
        ShadowBounds::cube(35.0),
    ));

    let point_spot_mult = 1000.0;

    // Sun Ground Refl
    for t in [
        Transform::from_xyz(2.0, 0.5, 1.5),
        Transform::from_xyz(-1.5, 0.5, 1.5),
        Transform::from_xyz(-5.0, 0.5, 1.5),
    ] {
        commands.spawn((
            t.looking_at(Vec3::new(0.0, 999.0, 0.0), Vec3::X),
            SpotLight {
                range: 15.0,
                radius: 4.0,
                intensity: 1000.0 * point_spot_mult,
                color: Color::srgb(1.0, 0.8, 0.7),
                shadows_enabled: false,
                inner_angle: PI * 0.4,
                outer_angle: PI * 0.5,
                ..default()
            },
        ));
    }

    // Sun Table Refl
    for t in [
        Transform::from_xyz(2.95, 0.5, 3.15),
        Transform::from_xyz(-6.2, 0.5, 2.3),
    ] {
        commands.spawn((
            t.looking_at(Vec3::new(0.0, 999.0, 0.0), Vec3::X),
            SpotLight {
                range: 3.0,
                radius: 1.5,
                intensity: 150.0 * point_spot_mult,
                color: Color::srgb(1.0, 0.9, 0.8),
                shadows_enabled: false,
                inner_angle: PI * 0.4,
                outer_angle: PI * 0.5,
                ..default()
            },
        ));
    }
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

        // Remove any lights or camera in the gltf scene
        if lights.get(entity).is_ok() || cameras.get(entity).is_ok() {
            commands.entity(entity).despawn();
        }
    }
}

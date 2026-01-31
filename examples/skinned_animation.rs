use std::f32::consts::PI;

use argh::FromArgs;
use bevy::{
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    light::light_consts::lux::DIRECT_SUNLIGHT,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    scene::SceneInstanceReady,
    window::PresentMode,
    winit::WinitSettings,
};
use bevy_mod_mipmap_generator::{MipmapGeneratorPlugin, generate_mipmaps};
use bgl2::{
    bevy_standard_lighting::OpenGLStandardLightingPlugin,
    bevy_standard_material::{OpenGLStandardMaterialPlugin, ReadReflection, SkipReflection},
    phase_shadow::ShadowBounds,
    plane_reflect::ReflectionPlane,
    render::OpenGLRenderPlugins,
};

#[derive(FromArgs, Resource, Clone, Default)]
/// Config
pub struct Args {
    /// use default bevy render backend (Also need to enable default plugins)
    #[argh(switch)]
    bevy: bool,
}

fn main() {
    #[cfg(target_arch = "wasm32")]
    let args: Args = Default::default();
    #[cfg(not(target_arch = "wasm32"))]
    let args: Args = argh::from_env();

    let mut app = App::new();
    app.insert_resource(args.clone())
        .insert_resource(ClearColor(Color::BLACK))
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
        .add_systems(Update, generate_mipmaps::<StandardMaterial>)
        .run();
}

const FOX_PATH: &str = "models/animated/Fox.glb";

#[derive(Component)]
struct AnimationToPlay {
    graph_handle: Handle<AnimationGraph>,
    index: AnimationNodeIndex,
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-2.5, 1.7, -1.0).looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
        EnvironmentMapLight {
            diffuse_map: asset_server.load("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
            specular_map: asset_server.load("environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
            intensity: 500.0,
            ..default()
        },
        FreeCamera::default(),
    ));

    let (graph, index) = AnimationGraph::from_clip(
        asset_server.load(GltfAssetLabel::Animation(2).from_asset(FOX_PATH)),
    );
    let graph_handle = graphs.add(graph);
    let animation_to_play = AnimationToPlay {
        graph_handle,
        index,
    };
    let mesh_scene = SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(FOX_PATH)));
    commands
        .spawn((
            animation_to_play,
            mesh_scene,
            Transform::from_scale(Vec3::ONE * 0.01),
        ))
        .observe(play_animation_when_ready);

    // Reflection plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(500.0, 500.0))),
        Transform::from_translation(vec3(0.0, 0.0, 0.0)),
        ReflectionPlane::default(),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::linear_rgba(0.0, 0.0, 0.0, 0.8),
            perceptual_roughness: 0.1,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        SkipReflection,
        ReadReflection,
    ));

    // Sun
    commands.spawn((
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, PI * -0.43, PI * -0.08, 0.0)),
        DirectionalLight {
            color: Color::srgb(1.0, 0.9, 0.8),
            illuminance: DIRECT_SUNLIGHT,
            shadows_enabled: true,
            shadow_depth_bias: 0.3,
            shadow_normal_bias: 1.0,
            ..default()
        },
        ShadowBounds::cube(1.0),
    ));
}

fn play_animation_when_ready(
    scene_ready: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    animations_to_play: Query<&AnimationToPlay>,
    mut players: Query<&mut AnimationPlayer>,
) {
    if let Ok(animation_to_play) = animations_to_play.get(scene_ready.entity) {
        for child in children.iter_descendants(scene_ready.entity) {
            if let Ok(mut player) = players.get_mut(child) {
                player.play(animation_to_play.index).repeat();
                commands
                    .entity(child)
                    .insert(AnimationGraphHandle(animation_to_play.graph_handle.clone()));
            }
        }
    }
}

//! Loads and renders a glTF file as a scene.

use argh::FromArgs;
use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    window::PresentMode,
    winit::WinitSettings,
};
use bevy_mod_mipmap_generator::{MipmapGeneratorPlugin, generate_mipmaps};
use bgl2::{
    bevy_standard_lighting::OpenGLStandardLightingPlugin,
    bevy_standard_material::OpenGLStandardMaterialPlugin, phase_shadow::ShadowBounds,
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

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(1.0, 0.4, 1.3).looking_at(Vec3::new(0.0, 0.2, 0.0), Vec3::Y),
        EnvironmentMapLight {
            diffuse_map: asset_server.load("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
            specular_map: asset_server.load("environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
            intensity: 250.0,
            ..default()
        },
    ));

    commands.spawn((
        Transform::default().looking_at(Vec3::new(0.5, -0.6, 0.3), Vec3::Y),
        DirectionalLight {
            shadows_enabled: true,
            shadow_depth_bias: 0.3,
            shadow_normal_bias: 0.6,
            ..default()
        },
        ShadowBounds::cube(2.0),
    ));
    commands.spawn(SceneRoot(asset_server.load(
        GltfAssetLabel::Scene(0).from_asset("models/FlightHelmet/FlightHelmet.gltf"),
    )));
    commands.spawn(SceneRoot(
        asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/Wood/wood.gltf")),
    ));
}

use bevy::{
    asset::RenderAssetUsages,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    window::PresentMode,
    winit::WinitSettings,
};
use bgl2::{
    UniformSet,
    bevy_standard_lighting::{
        DEFAULT_MAX_LIGHTS_DEF, OpenGLStandardLightingPlugin, StandardLightingUniforms,
    },
    bevy_standard_material::{OpenGLStandardMaterialPlugin, ViewUniforms},
    command_encoder::CommandEncoder,
    phase_shadow::{DirectionalLightShadow, ShadowBounds},
    prepare_image::{GpuImages, TextureRef},
    prepare_mesh::GpuMeshes,
    render::{OpenGLRenderPlugins, RenderPhase, RenderSet, register_render_system},
};
use uniform_set_derive::UniformSet;
use wgpu_types::{Extent3d, TextureDimension, TextureFormat};

fn main() {
    let mut app = App::new();
    app.insert_resource(WinitSettings::continuous())
        .add_plugins((
            default_plugins_no_render_backend().set(WindowPlugin {
                primary_window: Some(Window {
                    present_mode: PresentMode::Immediate,
                    ..default()
                }),
                ..default()
            }),
            OpenGLRenderPlugins,
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
            OpenGLStandardLightingPlugin,
            OpenGLStandardMaterialPlugin,
        ));

    register_render_system::<StandardMaterial, _>(app.world_mut(), render_custom_mat);

    app.add_systems(Startup, setup.in_set(RenderSet::Pipeline))
        .run();
}

fn default_plugins_no_render_backend() -> bevy::app::PluginGroupBuilder {
    DefaultPlugins.set(RenderPlugin {
        render_creation: WgpuSettings {
            backends: None,
            ..default()
        }
        .into(),
        ..default()
    })
}

/// set up a simple 3D scene
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut enc: ResMut<CommandEncoder>,
    asset_server: Res<AssetServer>,
) {
    let sphere_h = meshes.add(Sphere::new(0.33).mesh().uv(32, 18));
    for x in -5..5 {
        for z in -5..5 {
            let p = vec3(x as f32, 0.0, z as f32);
            let color = (p + 5.0) / 20.0;
            let linear_rgb = LinearRgba::rgb(color.x, color.y, color.z);
            let material_id = commands
                .spawn(CustomMaterial {
                    perceptual_roughness: 1.0 - (p.x + 5.0) / 10.0,
                    metallic: 1.0 - (p.z + 5.0) / 10.0,
                    color_texture: enc.bevy_image(create_test_image(linear_rgb.to_u8_array())),
                })
                .id();
            commands.spawn((
                Mesh3d(sphere_h.clone()),
                Transform::from_translation(p),
                CustomMaterialHandle(material_id),
            ));
        }
    }
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(7.0, 7.0, 7.0).looking_at(vec3(0.0, -2.0, 0.0), Vec3::Y),
        EnvironmentMapLight {
            diffuse_map: asset_server.load("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
            specular_map: asset_server.load("environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
            intensity: 2000.0,
            ..default()
        },
    ));

    let material_id = commands
        .spawn(CustomMaterial {
            perceptual_roughness: 0.5,
            metallic: 0.0,
            color_texture: enc.bevy_image(create_test_image([0, 0, 0, 255])),
        })
        .id();
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(50.0, 50.0))),
        Transform::from_translation(vec3(0.0, 0.0, 0.0)),
        CustomMaterialHandle(material_id),
    ));

    commands.spawn((
        Transform::default().looking_at(Vec3::new(0.0, -1.0, -2.0), Vec3::Y),
        DirectionalLight {
            shadows_enabled: true,
            shadow_depth_bias: 0.3,
            shadow_normal_bias: 0.6,
            ..default()
        },
        ShadowBounds::cube(15.0),
    ));
}

fn create_test_image(color: [u8; 4]) -> Image {
    Image::new(
        Extent3d {
            width: 1,
            height: 1,
            ..default()
        },
        TextureDimension::D2,
        color.to_vec(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::all(),
    )
}

#[derive(UniformSet, Component, Clone)]
#[uniform_set(prefix = "ub_")]
struct CustomMaterial {
    perceptual_roughness: f32,
    metallic: f32,
    color_texture: TextureRef,
}

#[derive(Component, Deref, DerefMut)]
struct CustomMaterialHandle(Entity);

fn render_custom_mat(
    mesh_entities: Query<(
        &ViewVisibility,
        &GlobalTransform,
        &Mesh3d,
        &CustomMaterialHandle,
    )>,
    materials: Query<&CustomMaterial>,
    phase: If<Res<RenderPhase>>,
    mut enc: ResMut<CommandEncoder>,
    shadow: Option<Res<DirectionalLightShadow>>,
) {
    let phase = **phase;

    let mut draws = Vec::new();

    struct DrawData {
        world_from_local: Mat4,
        material: CustomMaterial,
        mesh: AssetId<Mesh>,
    }

    for (view_vis, transform, mesh, material_h) in mesh_entities.iter() {
        if !view_vis.get() {
            continue;
        }

        let Ok(material) = materials.get(**material_h) else {
            continue;
        };
        let world_from_local = transform.to_matrix();

        draws.push(DrawData {
            world_from_local,
            material: material.clone(),
            mesh: mesh.id(),
        });
    }
    let shadow = shadow.as_deref().cloned();

    enc.record(move |ctx, world| {
        let shader_index = bgl2::shader_cached!(
            ctx,
            "../assets/shaders/custom_pbr_material.vert",
            "../assets/shaders/custom_pbr_material.frag",
            [DEFAULT_MAX_LIGHTS_DEF].iter().chain(
                world
                    .resource::<StandardLightingUniforms>()
                    .shader_defs(true, shadow.is_some(), &phase)
                    .iter()
            ),
            &[
                ViewUniforms::bindings(),
                StandardLightingUniforms::bindings(),
                CustomMaterial::bindings()
            ]
        )
        .unwrap();

        world.resource_mut::<GpuMeshes>().reset_mesh_bind_cache();
        ctx.use_cached_program(shader_index);

        ctx.map_uniform_set_locations::<CustomMaterial>();
        ctx.map_uniform_set_locations::<ViewUniforms>();

        ctx.bind_uniforms_set(
            world.resource::<GpuImages>(),
            world.resource::<ViewUniforms>(),
        );
        if !phase.depth_only() {
            ctx.map_uniform_set_locations::<StandardLightingUniforms>();
            ctx.bind_uniforms_set(
                world.resource::<GpuImages>(),
                world.resource::<StandardLightingUniforms>(),
            );
        }

        for draw in &draws {
            ctx.load("ub_world_from_local", draw.world_from_local);
            ctx.bind_uniforms_set(world.resource::<GpuImages>(), &draw.material);
            world
                .resource_mut::<GpuMeshes>()
                .draw_mesh(ctx, draw.mesh, shader_index);
        }
    });
}

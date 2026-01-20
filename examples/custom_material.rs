use bevy::{
    asset::RenderAssetUsages,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    window::PresentMode,
    winit::WinitSettings,
};
use bevy_opengl::{
    BevyGlContext,
    prepare_image::{GpuImages, bevy_image_to_gl_texture},
    prepare_mesh::GPUMeshBufferMap,
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
fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, ctx: NonSend<BevyGlContext>) {
    for x in -10..10 {
        for y in -10..10 {
            for z in -10..10 {
                let p = vec3(x as f32, y as f32, z as f32);
                let color = (p + 10.0) / 20.0;
                let linear_rgb = LinearRgba::rgb(color.x, color.y, color.z);
                let bevy_image = create_test_image(linear_rgb.to_u8_array());
                let material_id = commands
                    .spawn(CustomMaterial {
                        color: linear_rgb.to_vec4(),
                        emissive: bevy_image_to_gl_texture(&ctx, None, &bevy_image).unwrap().0,
                    })
                    .id();
                commands.spawn((
                    Mesh3d(meshes.add(Cuboid::default())),
                    Transform::from_translation(p),
                    CustomMaterialHandle(material_id),
                ));
            }
        }
    }
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(23.0, 23.0, 23.0).looking_at(vec3(0.0, -2.5, 0.0), Vec3::Y),
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

#[derive(Component, UniformSet)]
struct CustomMaterial {
    color: Vec4,
    emissive: glow::Texture,
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
    camera: Single<(Entity, &Camera, &GlobalTransform, &Projection)>,
    mut ctx: NonSendMut<BevyGlContext>,
    mut gpu_meshes: NonSendMut<GPUMeshBufferMap>,
    materials: Query<&CustomMaterial>,
    phase: If<Res<RenderPhase>>,
    gpu_images: NonSend<GpuImages>,
) {
    let (_entity, _camera, cam_global_trans, cam_proj) = *camera;
    let phase = **phase;

    let clip_from_world = match phase {
        RenderPhase::Opaque => {
            cam_proj.get_clip_from_view() * cam_global_trans.to_matrix().inverse()
        }
        _ => {
            return;
        }
    };

    let shader_index = bevy_opengl::shader_cached!(
        ctx,
        "../assets/shaders/custom_material.vert",
        "../assets/shaders/custom_material.frag",
        &[]
    )
    .unwrap();

    gpu_meshes.reset_bind_cache();
    ctx.use_cached_program(shader_index);

    ctx.map_uniform_set_locations::<CustomMaterial>();

    for (view_vis, transform, mesh, material_h) in mesh_entities.iter() {
        if !view_vis.get() {
            continue;
        }

        let Ok(material) = materials.get(**material_h) else {
            continue;
        };
        let world_from_local = transform.to_matrix();
        let clip_from_local = clip_from_world * world_from_local;

        ctx.load("clip_from_local", clip_from_local);

        ctx.bind_uniforms_set(&gpu_images, material);
        gpu_meshes.draw_mesh(&ctx, mesh.id(), shader_index);
    }
}

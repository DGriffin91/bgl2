use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    window::PresentMode,
    winit::WinitSettings,
};
use bevy_opengl::{
    BevyGlContext, load_val,
    prepare_image::GpuImages,
    prepare_mesh::GPUMeshBufferMap,
    queue_val,
    render::{OpenGLRenderPlugin, RenderPhase, RenderRunner, RenderSet},
    uniform_slot_builder::UniformSlotBuilder,
};

fn main() {
    let mut app = App::new();
    app.insert_resource(WinitSettings::continuous())
        .add_plugins((
            DefaultPlugins
                .set(RenderPlugin {
                    render_creation: WgpuSettings {
                        backends: None,
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
            OpenGLRenderPlugin,
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
        ));

    {
        let world = app.world_mut();
        let render_std_mat_id = world.register_system(render_std_mat);
        world
            .get_resource_mut::<RenderRunner>()
            .unwrap()
            .register::<StandardMaterial>(render_std_mat_id);
    }

    app.add_systems(Startup, setup.in_set(RenderSet::Pipeline))
        .run();
}

/// set up a simple 3D scene
fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    for x in -10..10 {
        for y in -10..10 {
            for z in -10..10 {
                let p = vec3(x as f32, y as f32, z as f32);
                let color = (p + 10.0) / 20.0;
                let linear_rgb = LinearRgba::rgb(color.x, color.y, color.z);
                let material_id = commands
                    .spawn(CustomMaterial {
                        color: linear_rgb.to_vec4(),
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

#[derive(Component)]
struct CustomMaterial {
    color: Vec4,
}

#[derive(Component, Deref, DerefMut)]
struct CustomMaterialHandle(Entity);

fn render_std_mat(
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
        RenderPhase::Shadow | RenderPhase::Transparent => {
            return;
        }
        RenderPhase::Opaque => {
            cam_proj.get_clip_from_view() * cam_global_trans.to_matrix().inverse()
        }
    };

    let shader_index =
        bevy_opengl::shader_cached!(ctx, "custom_material.vert", "custom_material.frag", &[])
            .unwrap();

    gpu_meshes.reset_bind_cache();
    ctx.use_cached_program(shader_index);

    let mut build = UniformSlotBuilder::<CustomMaterial>::new(&ctx, &gpu_images, shader_index);

    queue_val!(build, color);

    for (view_vis, transform, mesh, material_h) in mesh_entities.iter() {
        if !view_vis.get() {
            continue;
        }

        let Ok(material) = materials.get(**material_h) else {
            continue;
        };
        let world_from_local = transform.to_matrix();
        let clip_from_local = clip_from_world * world_from_local;

        load_val!(build, clip_from_local);

        build.run(material);
        gpu_meshes.draw_mesh(&ctx, mesh.id(), shader_index);
    }
}

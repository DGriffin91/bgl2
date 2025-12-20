use bevy::{
    ecs::system::SystemState,
    light::CascadeShadowConfigBuilder,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    winit::WINIT_WINDOWS,
};
use bevy_opengl::{
    BevyGlContext,
    prepare_mesh::{GPUMeshBufferMap, send_standard_meshes_to_gpu},
};
use glow::HasContext;

fn main() {
    //unsafe {
    //    std::env::set_var(
    //        "__EGL_VENDOR_LIBRARY_FILENAMES",
    //        "/usr/share/glvnd/egl_vendor.d/50_mesa.json",
    //    );
    //    std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
    //    std::env::set_var("MESA_LOADER_DRIVER_OVERRIDE", "llvmpipe");
    //    std::env::set_var("MESA_GL_VERSION_OVERRIDE", "2.1");
    //    std::env::set_var("MESA_GLSL_VERSION_OVERRIDE", "120");
    //}

    App::new()
        .init_resource::<bevy_opengl::prepare_mesh::GPUMeshBufferMap>()
        .add_plugins((DefaultPlugins.set(RenderPlugin {
            render_creation: WgpuSettings {
                backends: None,
                ..default()
            }
            .into(),
            ..default()
        }),))
        .add_systems(Startup, (setup, init))
        .add_systems(Update, (send_standard_meshes_to_gpu, update))
        .run();
}

/// set up a simple 3D scene
fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.7, 0.7, 1.0).looking_at(Vec3::new(0.0, 0.3, 0.0), Vec3::Y),
        EnvironmentMapLight {
            diffuse_map: asset_server.load("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
            specular_map: asset_server.load("environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
            intensity: 250.0,
            ..default()
        },
    ));

    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        // This is a relatively small scene, so use tighter shadow
        // cascade bounds than the default for better quality.
        // We also adjusted the shadow map to be larger since we're
        // only using a single cascade.
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            maximum_distance: 1.6,
            ..default()
        }
        .build(),
    ));
    commands.spawn(SceneRoot(asset_server.load(
        GltfAssetLabel::Scene(0).from_asset("models/FlightHelmet/FlightHelmet.gltf"),
    )));
}

fn init(world: &mut World, params: &mut SystemState<Query<(Entity, &mut Window)>>) {
    if world.contains_non_send::<BevyGlContext>() {
        return;
    }
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let mut windows = params.get_mut(world);

        let (bevy_window_entity, bevy_window) = windows.single_mut().unwrap();
        let Some(winit_window) = winit_windows.get_window(bevy_window_entity) else {
            warn!("No Window Found");
            return;
        };

        let ctx = BevyGlContext::new(&bevy_window, winit_window);

        world.insert_non_send_resource(ctx);
    });
}

fn update(
    mut mesh_entities: Query<(Entity, &ViewVisibility, Ref<GlobalTransform>, Ref<Mesh3d>)>,
    camera: Single<(Entity, &Camera, &GlobalTransform, &Projection)>,
    mut ctx: NonSendMut<BevyGlContext>,
    gpu_meshes: Res<GPUMeshBufferMap>,
) {
    let (_entity, _camera, cam_global_trans, cam_proj) = *camera;

    let _view_position = cam_global_trans.translation();
    let view_to_clip = cam_proj.get_clip_from_view();
    let view_to_world = cam_global_trans.to_matrix();
    let world_to_view = view_to_world.inverse();

    let clip_to_view = view_to_clip.inverse();

    let world_to_clip = view_to_clip * world_to_view;
    let _clip_to_world = view_to_world * clip_to_view;

    let vertex = r#"
attribute vec3 a_position;
attribute vec3 a_normal;
uniform mat4 mvp;
uniform mat4 local_to_world;
varying vec3 normal;

void main() {
    gl_Position = mvp * vec4(a_position, 1.0);
    normal = (local_to_world * vec4(a_normal, 0.0)).xyz;
}
    "#;

    let fragment = r#"
varying vec3 normal;
void main() {
    gl_FragColor = vec4(abs(normal), 1.0);
}
"#;

    let shader_index = ctx.shader_cached(
        vertex,
        fragment,
        Some(|context, program| unsafe {
            context.bind_attrib_location(program, 0, "a_position");
            context.bind_attrib_location(program, 1, "a_normal");
        }),
    );
    let mvp_loc = ctx.get_uniform_location(shader_index, "mvp").unwrap();
    let local_to_world_loc = ctx
        .get_uniform_location(shader_index, "local_to_world")
        .unwrap();

    ctx.use_cached_program(shader_index);
    unsafe {
        ctx.gl.clear_color(0.0, 0.0, 0.0, 1.0);
        ctx.gl.clear_depth_f32(0.0);
        ctx.gl
            .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        ctx.gl.enable(glow::DEPTH_TEST);
        ctx.gl.depth_func(glow::GREATER);
    };

    for (_entity, view_vis, transform, mesh) in &mut mesh_entities {
        if !view_vis.get() {
            continue;
        }
        if let Some(buffers) = gpu_meshes.buffers.get(&mesh.id()) {
            let local_to_world = transform.to_matrix();
            let local_to_clip = world_to_clip * local_to_world;
            unsafe {
                ctx.gl.uniform_matrix_4_f32_slice(
                    Some(&mvp_loc),
                    false,
                    &local_to_clip.to_cols_array(),
                );

                ctx.gl.uniform_matrix_4_f32_slice(
                    Some(&local_to_world_loc),
                    false,
                    &local_to_world.to_cols_array(),
                );

                ctx.gl.bind_vertex_array(Some(buffers.vertex));

                ctx.gl.draw_elements(
                    glow::TRIANGLES,
                    buffers.index_count as i32,
                    glow::UNSIGNED_INT,
                    0,
                );
                ctx.gl.bind_vertex_array(None);
            };
        }
    }
    ctx.swap();
}

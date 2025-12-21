use bevy::{
    ecs::system::SystemState,
    light::CascadeShadowConfigBuilder,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    winit::WINIT_WINDOWS,
};
use bevy_opengl::{
    BevyGlContext,
    prepare_image::GpuImages,
    prepare_mesh::GPUMeshBufferMap,
    render::{OpenGLRenderPlugin, RenderSet},
};
use glow::{Context, HasContext};

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
        .add_plugins((
            DefaultPlugins.set(RenderPlugin {
                render_creation: WgpuSettings {
                    backends: None,
                    ..default()
                }
                .into(),
                ..default()
            }),
            OpenGLRenderPlugin,
        ))
        .add_systems(Startup, (setup, init))
        .add_systems(Update, update.in_set(RenderSet::Render))
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
    mut mesh_entities: Query<(
        Entity,
        &ViewVisibility,
        Ref<GlobalTransform>,
        Ref<Mesh3d>,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    camera: Single<(Entity, &Camera, &GlobalTransform, &Projection)>,
    mut ctx: NonSendMut<BevyGlContext>,
    gpu_meshes: Res<GPUMeshBufferMap>,
    materials: Res<Assets<StandardMaterial>>,
    gpu_images: Res<GpuImages>,
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
attribute vec2 a_uv_0;
attribute vec2 a_uv_1;

uniform mat4 mvp;
uniform mat4 local_to_world;

varying vec3 normal;
varying vec2 uv_0;
varying vec2 uv_1;

void main() {
    gl_Position = mvp * vec4(a_position, 1.0);
    normal = (local_to_world * vec4(a_normal, 0.0)).xyz;
    uv_0 = a_uv_0;
    uv_1 = a_uv_1;
}
    "#;

    let fragment = r#"
varying vec3 normal;
varying vec2 uv_0;
varying vec2 uv_1;

uniform sampler2D color_texture;

void main() {
    gl_FragColor = texture2D(color_texture, uv_0);
}
"#;

    let a_position_index = 0;
    let a_normal_index = 1;
    let a_uv_0_index = 2;
    let a_uv_1_index = 3;
    let shader_index = ctx.shader_cached(vertex, fragment, |context: &Context, program| unsafe {
        context.bind_attrib_location(program, a_position_index, "a_position");
        context.bind_attrib_location(program, a_normal_index, "a_normal");
        context.bind_attrib_location(program, a_uv_0_index, "a_uv_0");
        context.bind_attrib_location(program, a_uv_1_index, "a_uv_1");
    });
    let mvp_loc = ctx.get_uniform_location(shader_index, "mvp");
    let local_to_world_loc = ctx.get_uniform_location(shader_index, "local_to_world");
    let color_texture_loc = ctx.get_uniform_location(shader_index, "color_texture");

    ctx.use_cached_program(shader_index);
    unsafe {
        ctx.gl.clear_color(0.0, 0.0, 0.0, 1.0);
        ctx.gl.clear_depth_f32(0.0);
        ctx.gl
            .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        ctx.gl.enable(glow::DEPTH_TEST);
        ctx.gl.depth_func(glow::GREATER);
    };

    for (_entity, view_vis, transform, mesh, material_h) in &mut mesh_entities {
        if !view_vis.get() {
            continue;
        }
        let Some(material) = materials.get(material_h) else {
            continue;
        };
        if let Some(buffers) = gpu_meshes.buffers.get(&mesh.id()) {
            let local_to_world = transform.to_matrix();
            let local_to_clip = world_to_clip * local_to_world;
            unsafe {
                ctx.gl
                    .bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(buffers.index));

                ctx.gl
                    .bind_buffer(glow::ARRAY_BUFFER, Some(buffers.position));
                ctx.gl.vertex_attrib_pointer_f32(
                    a_position_index,
                    3,
                    glow::FLOAT,
                    false,
                    3 * size_of::<f32>() as i32,
                    0,
                );
                ctx.gl.enable_vertex_attrib_array(a_position_index);

                if let Some(normal) = buffers.normal {
                    ctx.gl.bind_buffer(glow::ARRAY_BUFFER, Some(normal));
                    ctx.gl.vertex_attrib_pointer_f32(
                        a_normal_index,
                        3,
                        glow::FLOAT,
                        false,
                        3 * size_of::<f32>() as i32,
                        0,
                    );
                    ctx.gl.enable_vertex_attrib_array(a_normal_index);
                }

                if let Some(uv_0) = buffers.uv_0 {
                    ctx.gl.bind_buffer(glow::ARRAY_BUFFER, Some(uv_0));
                    ctx.gl.vertex_attrib_pointer_f32(
                        a_uv_0_index,
                        2,
                        glow::FLOAT,
                        false,
                        2 * size_of::<f32>() as i32,
                        0,
                    );
                    ctx.gl.enable_vertex_attrib_array(a_uv_0_index);
                }

                if let Some(uv_1) = buffers.uv_1 {
                    ctx.gl.bind_buffer(glow::ARRAY_BUFFER, Some(uv_1));
                    ctx.gl.vertex_attrib_pointer_f32(
                        a_uv_1_index,
                        2,
                        glow::FLOAT,
                        false,
                        2 * size_of::<f32>() as i32,
                        0,
                    );
                    ctx.gl.enable_vertex_attrib_array(a_uv_1_index);
                }

                if let Some(mvp_loc) = mvp_loc {
                    ctx.gl.uniform_matrix_4_f32_slice(
                        Some(&mvp_loc),
                        false,
                        &local_to_clip.to_cols_array(),
                    );
                }

                if let Some(local_to_world_loc) = local_to_world_loc {
                    ctx.gl.uniform_matrix_4_f32_slice(
                        Some(&local_to_world_loc),
                        false,
                        &local_to_world.to_cols_array(),
                    );
                }

                // TODO fallback texture
                if let Some(color_texture_loc) = color_texture_loc {
                    if let Some(ref image_h) = material.base_color_texture {
                        if let Some(index) = gpu_images.mapping.get(&image_h.id()) {
                            let texture = gpu_images.images[*index as usize];
                            ctx.gl.active_texture(glow::TEXTURE0);
                            ctx.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                            ctx.gl.uniform_1_i32(Some(&color_texture_loc), 0);
                        }
                    }
                }

                ctx.gl.draw_elements(
                    glow::TRIANGLES,
                    buffers.index_count as i32,
                    glow::UNSIGNED_SHORT, // Base ES 2.0 and WebGL 1.0 only support GL_UNSIGNED_BYTE or GL_UNSIGNED_SHORT
                    0,
                );
                ctx.gl.bind_vertex_array(None);
            };
        }
    }
    ctx.swap();
}

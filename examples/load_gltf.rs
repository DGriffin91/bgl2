use bevy::{
    ecs::system::SystemState,
    light::CascadeShadowConfigBuilder,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    winit::WINIT_WINDOWS,
};
use bevy_opengl::{
    BevyGlContext, if_loc,
    prepare_image::GpuImages,
    prepare_mesh::GPUMeshBufferMap,
    render::{OpenGLRenderPlugin, RenderSet},
    unifrom_slot_builder::UniformSlotBuilder,
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
attribute vec4 Vertex_Tangent;
attribute vec3 Vertex_Position;
attribute vec3 Vertex_Normal;
attribute vec2 Vertex_Uv;
attribute vec2 Vertex_Uv_1;

uniform mat4 mvp;
uniform mat4 local_to_world;

varying vec4 tangent;
varying vec3 normal;
varying vec2 uv_0;
varying vec2 uv_1;

void main() {
    gl_Position = mvp * vec4(Vertex_Position, 1.0);
    normal = (local_to_world * vec4(Vertex_Normal, 0.0)).xyz;
    uv_0 = Vertex_Uv;
    uv_1 = Vertex_Uv_1;
    tangent = Vertex_Tangent;
}
    "#;

    let fragment = r#"
varying vec4 tangent;
varying vec3 normal;
varying vec2 uv_0;
varying vec2 uv_1;

uniform vec4 base_color;

uniform bool double_sided;
uniform bool flip_normal_map_y;
uniform int flags;

uniform sampler2D color_texture;
uniform sampler2D normal_texture;


// http://www.mikktspace.com/
vec3 apply_normal_mapping(vec3 ws_normal, vec4 ws_tangent, vec2 uv) {
    vec3 N = ws_normal;
    vec3 T = ws_tangent.xyz;
    vec3 B = ws_tangent.w * cross(N, T);
    vec3 Nt = texture2D(normal_texture, uv).rgb * 2.0 - 1.0; // Only supports 3-component normal maps
    if (flip_normal_map_y) {
        Nt.y = -Nt.y;
    }
    if (double_sided && !gl_FrontFacing) {
        Nt = -Nt;
    }
    N = Nt.x * T + Nt.y * B + Nt.z * N;
    return normalize(N);
}

void main() {
    vec3 light_dir = normalize(vec3(-0.2, 0.2, 1.0));
    vec4 color = base_color * texture2D(color_texture, uv_0);
    vec3 normal = apply_normal_mapping(normal, tangent, uv_0);
    float light = max(dot(light_dir, normal), 0.0);
    gl_FragColor = vec4(color.rgb * light, color.a);
}
"#;

    let shader_index = ctx.shader_cached(vertex, fragment, |_, _| {});
    let mvp_loc = ctx.get_uniform_location(shader_index, "mvp");
    let local_to_world_loc = ctx.get_uniform_location(shader_index, "local_to_world");

    let mut material_builder =
        UniformSlotBuilder::<StandardMaterial>::new(&ctx, &gpu_images, shader_index);

    material_builder.value("flip_normal_map_y", |ctx, material, loc| {
        ctx.uniform_bool(&loc, material.flip_normal_map_y)
    });
    material_builder.value("double_sided", |ctx, material, loc| {
        ctx.uniform_bool(&loc, material.double_sided)
    });
    material_builder.value("base_color", |ctx, material, loc| {
        ctx.uniform_vec4(&loc, material.base_color.to_linear().to_f32_array().into())
    });

    material_builder.texture("color_texture", |material| &material.base_color_texture);
    material_builder.texture("normal_texture", |material| &material.normal_map_texture);

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
            };

            buffers.bind(&ctx, shader_index);

            if_loc(&mvp_loc, |loc| ctx.uniform_mat4(&loc, &local_to_clip));
            if_loc(&local_to_world_loc, |loc| {
                ctx.uniform_mat4(&loc, &local_to_world)
            });

            material_builder.run(material);

            unsafe {
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

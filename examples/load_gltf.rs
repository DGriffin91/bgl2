use bevy::{
    asset::UnapprovedPathMode,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    ecs::system::SystemState,
    light::CascadeShadowConfigBuilder,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    window::PresentMode,
    winit::{UpdateMode, WINIT_WINDOWS, WinitSettings},
};
use bevy_basic_camera::{CameraController, CameraControllerPlugin};
use bevy_mod_mipmap_generator::{MipmapGeneratorPlugin, generate_mipmaps};
use bevy_opengl::{
    BevyGlContext,
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
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::Continuous,
            unfocused_mode: UpdateMode::Continuous,
        })
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
                .set(AssetPlugin {
                    // Allow scenes to be loaded from anywhere on disk
                    unapproved_path_mode: UnapprovedPathMode::Allow,
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
            CameraControllerPlugin,
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
        ))
        .add_plugins(MipmapGeneratorPlugin)
        .add_systems(Update, generate_mipmaps::<StandardMaterial>)
        .add_systems(Startup, (setup, init))
        .add_systems(PostUpdate, update.in_set(RenderSet::Render))
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
        CameraController {
            orbit_mode: true,
            orbit_focus: Vec3::new(0.0, 0.3, 0.0),
            ..default()
        },
    ));

    commands.spawn(SceneRoot(
        asset_server.load("models/bistro_exterior/BistroExterior.gltf#Scene0"),
    ));
    commands.spawn((
        SceneRoot(asset_server.load("models/bistro_interior_wine/BistroInterior_Wine.gltf#Scene0")),
        Transform::from_xyz(0.0, 0.3, -0.2),
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
    commands.spawn(SceneRoot(asset_server.load("models/Wood/wood.gltf#Scene0")));
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
    gpu_meshes: NonSend<GPUMeshBufferMap>,
    materials: Res<Assets<StandardMaterial>>,
    gpu_images: NonSend<GpuImages>,
) {
    let (_entity, _camera, cam_global_trans, cam_proj) = *camera;

    let view_position = cam_global_trans.translation();
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

uniform mat4 local_to_clip;
uniform mat4 local_to_world;
uniform mat4 view_to_world;

varying vec3 ws_position;
varying vec4 tangent;
varying vec3 normal;
varying vec2 uv_0;
varying vec2 uv_1;

void main() {
    gl_Position = local_to_clip * vec4(Vertex_Position, 1.0);
    normal = (local_to_world * vec4(Vertex_Normal, 0.0)).xyz;
    ws_position = (local_to_world * vec4(Vertex_Position, 1.0)).xyz;
    uv_0 = Vertex_Uv;
    uv_1 = Vertex_Uv_1;
    tangent = Vertex_Tangent;
}
    "#;

    let fragment = r#"
varying vec3 ws_position;
varying vec4 tangent;
varying vec3 normal;
varying vec2 uv_0;
varying vec2 uv_1;

uniform vec3 view_position;

uniform vec4 base_color;
uniform float perceptual_roughness;

uniform bool double_sided;
uniform bool flip_normal_map_y;
uniform bool alpha_blend;
uniform int flags;

uniform sampler2D color_texture;
uniform sampler2D normal_texture;
uniform sampler2D metallic_roughness_texture;

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
    vec3 light_dir = normalize(vec3(-0.2, 0.5, 1.0));
    vec3 light_color = vec3(1.0, 1.0, 1.0);
    float specular_intensity = 0.5;

    vec3 V = normalize(ws_position - view_position);
    vec3 view_dir = normalize(view_position - ws_position);

    vec4 metallic_roughness = texture2D(metallic_roughness_texture, uv_0);
    float roughness = metallic_roughness.b * perceptual_roughness;
    roughness *= roughness;

    vec4 color = base_color * texture2D(color_texture, uv_0);

    if (!alpha_blend && (color.a < 0.5)) {
        discard;
    }

    vec3 normal = apply_normal_mapping(normal, tangent, uv_0);

    // https://en.wikipedia.org/wiki/Blinn%E2%80%93Phong_reflection_model
    float lambert = dot(light_dir, normal);

    vec3 half_dir = normalize(light_dir + view_dir);
    float spec_angle = max(dot(half_dir, normal), 0.0);
    float shininess = mix(1.0, 32.0, (1.0 - roughness));
    float specular = pow(spec_angle, shininess);
    specular = specular * pow(min(lambert + 1.0, 1.0), 4.0); // Fade out spec TODO improve

    lambert = max(lambert, 0.0);
    gl_FragColor = vec4(color.rgb * lambert * light_color + specular * light_color * specular_intensity, color.a);
}
"#;

    let shader_index = ctx.shader_cached(vertex, fragment, |_, _| {});
    ctx.use_cached_program(shader_index);

    let mut build = UniformSlotBuilder::<StandardMaterial>::new(&ctx, &gpu_images, shader_index);

    build.val("flip_normal_map_y", |m| m.flip_normal_map_y);
    build.val("double_sided", |m| m.double_sided);
    build.val("alpha_blend", |m| material_alpha_blend(m));
    build.val("base_color", |m| m.base_color.to_linear().to_vec4());
    build.val("perceptual_roughness", |m| m.perceptual_roughness);

    build.tex("color_texture", |m| &m.base_color_texture);
    build.tex("normal_texture", |m| &m.normal_map_texture);
    build.tex("metallic_roughness_texture", |m| {
        &m.metallic_roughness_texture
    });

    build.upload("view_to_world", view_to_world);
    build.upload("view_position", view_position);

    unsafe {
        ctx.gl.depth_mask(true);
        ctx.gl.clear_color(0.0, 0.0, 0.0, 1.0);
        ctx.gl.clear_depth_f32(0.0);
        ctx.gl
            .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
    };

    for alpha_blend in [false, true] {
        if alpha_blend {
            unsafe {
                ctx.gl.enable(glow::DEPTH_TEST);
                ctx.gl.enable(glow::BLEND);
                ctx.gl.depth_func(glow::GREATER);
                ctx.gl.depth_mask(false);
                ctx.gl
                    .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            }
        } else {
            unsafe {
                ctx.gl.enable(glow::DEPTH_TEST);
                ctx.gl.disable(glow::BLEND);
                ctx.gl.depth_func(glow::GREATER);
                ctx.gl.depth_mask(true);
                ctx.gl.blend_func(glow::ZERO, glow::ONE);
            }
        }

        for (_entity, view_vis, transform, mesh, material_h) in &mut mesh_entities {
            if !view_vis.get() {
                continue;
            }
            let Some(material) = materials.get(material_h) else {
                continue;
            };
            if material_alpha_blend(material) != alpha_blend {
                continue;
            }
            if let Some(buffers) = gpu_meshes.buffers.get(&mesh.id()) {
                let local_to_world = transform.to_matrix();
                let local_to_clip = world_to_clip * local_to_world;
                unsafe {
                    ctx.gl
                        .bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(buffers.index));
                };

                buffers.bind(&ctx, shader_index);

                // TODO cache
                build.upload("local_to_clip", local_to_clip);
                build.upload("local_to_world", local_to_world);

                build.run(material);

                unsafe {
                    ctx.gl.draw_elements(
                        glow::TRIANGLES,
                        buffers.index_count as i32,
                        buffers.index_element_type,
                        0,
                    );
                    ctx.gl.bind_vertex_array(None);
                };
            }
        }
    }
}

fn material_alpha_blend(material: &StandardMaterial) -> bool {
    let material_blend = match material.alpha_mode {
        AlphaMode::Opaque => false,
        AlphaMode::Mask(_) => false,
        AlphaMode::Blend => true,
        AlphaMode::Premultiplied => true,
        AlphaMode::AlphaToCoverage => true,
        AlphaMode::Add => true,
        AlphaMode::Multiply => true,
    };
    material_blend
}

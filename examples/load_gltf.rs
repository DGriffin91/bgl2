use std::f32::consts::PI;

use bevy::{
    camera::primitives::Aabb,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    light::{CascadeShadowConfigBuilder, light_consts::lux},
    prelude::*,
    window::PresentMode,
    winit::WinitSettings,
};
use bevy_mod_mipmap_generator::{MipmapGeneratorPlugin, generate_mipmaps};
use bevy_opengl::{
    BevyGlContext, load_gl_tex, load_slice, load_tex, load_val,
    prepare_image::GpuImages,
    prepare_mesh::GPUMeshBufferMap,
    queue_tex, queue_val,
    render::{
        DeferredAlphaBlendDraws, DirectionalLightInfo, OpenGLRenderPlugin, RenderPhase, RenderSet,
        default_plugins_no_render_backend, register_render_system,
    },
    shader_cached,
    uniform_slot_builder::UniformSlotBuilder,
};
use itertools::Either;

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
            OpenGLRenderPlugin,
            FreeCameraPlugin,
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
        ));

    register_render_system::<StandardMaterial, _>(app.world_mut(), render_std_mat);

    app.add_plugins(MipmapGeneratorPlugin)
        .add_systems(Update, generate_mipmaps::<StandardMaterial>)
        .add_systems(Startup, setup.in_set(RenderSet::Pipeline))
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut ctx: NonSendMut<BevyGlContext>,
) {
    ctx.add_snippet("agx", include_str!("agx.glsl"));
    ctx.add_snippet("shadow_sampling", include_str!("shadow_sampling.glsl"));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.7, 0.7, 1.0).looking_at(Vec3::new(0.0, 0.3, 0.0), Vec3::Y),
        EnvironmentMapLight {
            diffuse_map: asset_server.load("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
            specular_map: asset_server.load("environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
            intensity: 250.0,
            ..default()
        },
        FreeCamera::default(),
    ));

    commands.spawn(SceneRoot(
        asset_server.load("models/bistro_exterior/BistroExterior.gltf#Scene0"),
    ));
    commands.spawn((
        SceneRoot(asset_server.load("models/bistro_interior_wine/BistroInterior_Wine.gltf#Scene0")),
        Transform::from_xyz(0.0, 0.3, -0.2),
    ));

    commands.spawn((
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, PI * -0.35, PI * -0.13, 0.0)),
        DirectionalLight {
            color: Color::srgb(1.0, 0.87, 0.78),
            illuminance: lux::FULL_DAYLIGHT,
            shadows_enabled: true,
            shadow_depth_bias: 0.2,
            shadow_normal_bias: 0.2,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 0.1,
            maximum_distance: 70.0,
            first_cascade_far_bound: 70.0,
            overlap_proportion: 0.2,
        }
        .build(),
    ));
    commands.spawn((
        PointLight {
            shadows_enabled: false,
            intensity: 10000.0,
            color: Color::linear_rgb(1.0, 0.0, 1.0),
            ..default()
        },
        Transform::from_xyz(1.0, 1.0, 1.0),
    ));
    commands.spawn(SceneRoot(asset_server.load(
        GltfAssetLabel::Scene(0).from_asset("models/FlightHelmet/FlightHelmet.gltf"),
    )));
    commands.spawn(SceneRoot(asset_server.load("models/Wood/wood.gltf#Scene0")));
}

fn render_std_mat(
    mesh_entities: Query<(
        Entity,
        &ViewVisibility,
        &GlobalTransform,
        &Mesh3d,
        &Aabb,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    camera: Single<(
        Entity,
        &Camera,
        &GlobalTransform,
        &Projection,
        Option<&EnvironmentMapLight>,
    )>,
    point_lights: Query<(&PointLight, &GlobalTransform)>,
    mut ctx: NonSendMut<BevyGlContext>,
    mut gpu_meshes: NonSendMut<GPUMeshBufferMap>,
    materials: Res<Assets<StandardMaterial>>,
    phase: If<Res<RenderPhase>>,
    mut transparent_draws: ResMut<DeferredAlphaBlendDraws>,
    shadow: Option<Res<DirectionalLightInfo>>,
    gpu_images: NonSend<GpuImages>,
    bevy_window: Single<&Window>,
    directional_lights: Query<&Transform, With<DirectionalLight>>,
) {
    let (_entity, _camera, cam_global_trans, cam_proj, env_light) = *camera;
    let phase = **phase;

    let view_position;
    let world_from_view;
    let view_from_world;
    let clip_from_world;
    let shadow_def;

    match phase {
        RenderPhase::Shadow => {
            if let Some(shadow) = &shadow {
                view_position = shadow.cascade.world_from_cascade.project_point3(Vec3::ZERO);
                //clip_from_view = shadow.cascade.clip_from_cascade;
                world_from_view = shadow.cascade.world_from_cascade;
                view_from_world = world_from_view.inverse();
                clip_from_world = shadow.cascade.clip_from_world;
                shadow_def = ("RENDER_SHADOW", "");
            } else {
                return;
            }
        }
        RenderPhase::Opaque | RenderPhase::Transparent => {
            view_position = cam_global_trans.translation();
            let clip_from_view = cam_proj.get_clip_from_view();
            world_from_view = cam_global_trans.to_matrix();
            view_from_world = world_from_view.inverse();
            clip_from_world = clip_from_view * view_from_world;
            shadow_def = shadow.as_ref().map_or(("", ""), |_| ("SAMPLE_SHADOW", ""));
        }
    }
    let shader_index =
        shader_cached!(ctx, "npr_std_mat.vert", "npr_std_mat.frag", &[shadow_def]).unwrap();
    gpu_meshes.reset_bind_cache();
    ctx.use_cached_program(shader_index);

    let mut build = UniformSlotBuilder::<StandardMaterial>::new(&ctx, &gpu_images, shader_index);

    queue_val!(build, flip_normal_map_y);
    queue_val!(build, double_sided);
    queue_val!(build, perceptual_roughness);
    queue_val!(build, metallic);

    queue_tex!(build, base_color_texture);
    queue_tex!(build, normal_map_texture);
    queue_tex!(build, metallic_roughness_texture);

    let env_light = env_light.unwrap();

    let specular_map = env_light.specular_map.clone();
    load_tex!(build, specular_map);
    let diffuse_map = env_light.diffuse_map.clone();
    load_tex!(build, diffuse_map);

    if let Some(shadow) = &shadow {
        let shadow_texture = shadow.texture.clone();
        load_gl_tex!(build, shadow_texture);
        let shadow_clip_from_world = shadow.cascade.clip_from_world;
        load_val!(build, shadow_clip_from_world);
    }

    if let Some(trans) = directional_lights.iter().next() {
        build.load("directional_light_dir_to_light", trans.back().as_vec3());
    } else {
        build.load("directional_light_dir_to_light", Vec3::ZERO);
    }

    build.queue_val("alpha_blend", |m| material_alpha_blend(m));
    build.queue_val("base_color", |m| m.base_color.to_linear().to_vec4());

    load_val!(build, world_from_view);
    load_val!(build, view_position);

    let view_resolution = vec2(
        bevy_window.physical_width().max(1) as f32,
        bevy_window.physical_height().max(1) as f32,
    );
    load_val!(build, view_resolution);

    let mut point_light_position_range = Vec::new();
    let mut point_light_color_radius = Vec::new();

    for (light, trans) in &point_lights {
        point_light_position_range.push(trans.translation().extend(light.range));
        point_light_color_radius.push(light.color.to_linear().to_vec3().extend(light.radius));
    }

    let light_count = point_light_position_range.len() as i32;
    load_val!(build, light_count);
    load_slice!(build, point_light_position_range);
    load_slice!(build, point_light_color_radius);

    let iter = match phase {
        RenderPhase::Shadow | RenderPhase::Opaque => Either::Left(mesh_entities.iter()),
        RenderPhase::Transparent => {
            Either::Right(mesh_entities.iter_many(transparent_draws.take()))
        }
    };

    for (entity, view_vis, transform, mesh, aabb, material_h) in iter {
        if phase != RenderPhase::Shadow && !view_vis.get() {
            continue;
        }

        let Some(material) = materials.get(material_h) else {
            continue;
        };
        let world_from_local = transform.to_matrix();
        let clip_from_local = clip_from_world * world_from_local;

        // If in opaque phase and must defer any alpha blend draws so they can be sorted and run in order.
        if material_alpha_blend(material) {
            if phase == RenderPhase::Opaque {
                let ws_radius = transform.radius_vec3a(aabb.half_extents);
                let ws_center = world_from_local.transform_point3a(aabb.center);
                transparent_draws.defer::<StandardMaterial>(
                    // Use closest point on bounding sphere
                    view_from_world.project_point3a(ws_center).z + ws_radius,
                    entity,
                );
            }
            if phase != RenderPhase::Transparent {
                continue;
            }
        }

        load_val!(build, world_from_local);
        load_val!(build, clip_from_local);

        build.run(material);
        gpu_meshes.draw_mesh(&ctx, mesh.id(), shader_index);
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

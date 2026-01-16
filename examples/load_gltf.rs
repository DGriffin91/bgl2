use std::f32::consts::PI;

use argh::FromArgs;
use bevy::{
    camera::{Exposure, primitives::Aabb},
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    core_pipeline::{prepass::DepthPrepass, tonemapping::Tonemapping},
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    light::{CascadeShadowConfigBuilder, light_consts::lux::DIRECT_SUNLIGHT},
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    scene::SceneInstanceReady,
    window::PresentMode,
    winit::WinitSettings,
};
use bevy_mod_mipmap_generator::{MipmapGeneratorPlugin, generate_mipmaps};
use bevy_opengl::{
    BevyGlContext, UniformValue, load_slice, load_tex, load_val,
    mesh_util::octahedral_encode,
    phase_shadow::DirectionalLightInfo,
    phase_transparent::DeferredAlphaBlendDraws,
    plane_reflect::{PlaneReflectionTexture, ReflectionPlane},
    prepare_image::GpuImages,
    prepare_mesh::GPUMeshBufferMap,
    queue_tex, queue_val,
    render::{
        OpenGLRenderPlugins, RenderPhase, RenderSet, register_prepare_system,
        register_render_system, set_blend_func_from_alpha_mode, transparent_draw_from_alpha_mode,
    },
    shader_cached,
    uniform_slot_builder::UniformSlotBuilder,
};
use itertools::Either;

#[derive(FromArgs, Resource, Clone, Default)]
/// Config
pub struct Args {
    /// render with npr shaders (non-physically based). Otherwise PBR shaders are used.
    #[argh(switch)]
    npr: bool,
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
        //.insert_resource(ClearColor(Color::srgb(1.75 * 0.5, 1.9 * 0.5, 1.99 * 0.5)))
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
        app.add_plugins(OpenGLRenderPlugins);
        register_prepare_system(app.world_mut(), prepare_view);
        register_render_system::<StandardMaterial, _>(app.world_mut(), render_std_mat);
    }

    app.add_systems(Update, generate_mipmaps::<StandardMaterial>)
        .add_systems(Startup, setup.in_set(RenderSet::Pipeline))
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut ctx: Option<NonSendMut<BevyGlContext>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if let Some(ctx) = &mut ctx {
        ctx.add_snippet("agx", include_str!("../assets/shaders/agx.glsl"));
        ctx.add_snippet(
            "std_mat_bindings",
            include_str!("../assets/shaders/std_mat_bindings.glsl"),
        );
        ctx.add_snippet(
            "shadow_sampling",
            include_str!("../assets/shaders/shadow_sampling.glsl"),
        );
        ctx.add_snippet("math", include_str!("../assets/shaders/math.glsl"));
        ctx.add_snippet("pbr", include_str!("../assets/shaders/pbr.glsl"));
    }

    // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-10.5, 1.7, -1.0).looking_at(Vec3::new(0.0, 2.5, 0.0), Vec3::Y),
        //Transform::from_xyz(12.5, 1.7, 12.0).looking_at(Vec3::new(0.0, 2.5, 0.0), Vec3::Y),
        Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::PI / 2.8,
            ..default()
        }),
        EnvironmentMapLight {
            diffuse_map: asset_server.load("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
            specular_map: asset_server.load("environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
            intensity: 500.0,
            ..default()
        },
        DepthPrepass,
        FreeCamera::default(),
        Tonemapping::TonyMcMapface,
    ));

    commands
        .spawn((
            SceneRoot(asset_server.load("models/san-miguel/san-miguel.gltf#Scene0")),
            Transform::from_xyz(-18.0, 0.0, 0.0),
        ))
        .observe(proc_scene);

    //commands.spawn((
    //    SceneRoot(
    //        asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/DamagedHelmet.glb")),
    //    ),
    //    Transform::from_scale(Vec3::ONE * 5.0).with_translation(vec3(0.0, 5.0, 0.0)),
    //));

    // Reflection plane
    //commands.spawn((
    //    Mesh3d(meshes.add(Plane3d::default().mesh().size(50.0, 50.0))),
    //    Transform::from_translation(vec3(0.0, 0.1, 0.0)),
    //    ReflectionPlane::default(),
    //    MeshMaterial3d(materials.add(StandardMaterial {
    //        base_color: Color::linear_rgba(0.0, 0.0, 0.0, 0.8),
    //        perceptual_roughness: 0.1,
    //        alpha_mode: AlphaMode::Blend,
    //        ..default()
    //    })),
    //    SkipReflection,
    //    ReadReflection,
    //));

    // Sun
    commands.spawn((
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, PI * -0.43, PI * -0.08, 0.0)),
        DirectionalLight {
            color: Color::srgb(1.0, 0.9, 0.8),
            illuminance: DIRECT_SUNLIGHT,
            shadows_enabled: true,
            shadow_depth_bias: 0.3,
            shadow_normal_bias: 0.7,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 0.1,
            maximum_distance: 25.0,
            first_cascade_far_bound: 70.0,
            overlap_proportion: 0.2,
        }
        .build(),
    ));

    let point_spot_mult = 1000.0;

    // Sun Ground Refl
    for t in [
        Transform::from_xyz(2.0, 0.5, 1.5),
        Transform::from_xyz(-1.5, 0.5, 1.5),
        Transform::from_xyz(-5.0, 0.5, 1.5),
    ] {
        commands.spawn((
            t.looking_at(Vec3::new(0.0, 999.0, 0.0), Vec3::X),
            SpotLight {
                range: 15.0,
                radius: 4.0,
                intensity: 1000.0 * point_spot_mult,
                color: Color::srgb(1.0, 0.8, 0.7),
                shadows_enabled: false,
                inner_angle: PI * 0.4,
                outer_angle: PI * 0.5,
                ..default()
            },
        ));
    }

    // Sun Table Refl
    for t in [
        Transform::from_xyz(2.95, 0.5, 3.15),
        Transform::from_xyz(-6.2, 0.5, 2.3),
    ] {
        commands.spawn((
            t.looking_at(Vec3::new(0.0, 999.0, 0.0), Vec3::X),
            SpotLight {
                range: 3.0,
                radius: 1.5,
                intensity: 150.0 * point_spot_mult,
                color: Color::srgb(1.0, 0.9, 0.8),
                shadows_enabled: false,
                inner_angle: PI * 0.4,
                outer_angle: PI * 0.5,
                ..default()
            },
        ));
    }
}

#[allow(clippy::type_complexity)]
pub fn proc_scene(
    scene_ready: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    has_std_mat: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lights: Query<Entity, Or<(With<PointLight>, With<DirectionalLight>, With<SpotLight>)>>,
    cameras: Query<Entity, With<Camera>>,
) {
    for entity in children.iter_descendants(scene_ready.entity) {
        // Sponza needs flipped normals
        if let Ok(mat_h) = has_std_mat.get(entity) {
            if let Some(mat) = materials.get_mut(mat_h) {
                mat.flip_normal_map_y = true;
            }
        }

        // Sponza has a bunch of lights by default
        if lights.get(entity).is_ok() {
            commands.entity(entity).despawn();
        }

        // Sponza has a bunch of cameras by default
        if cameras.get(entity).is_ok() {
            commands.entity(entity).despawn();
        }
    }
}

#[derive(Component, Default)]
struct SkipReflection;

#[derive(Component, Default)]
struct ReadReflection;

#[derive(Component, Clone)]
struct ViewUniforms {
    position: Vec3,
    world_from_view: Mat4,
    from_world: Mat4,
    clip_from_world: Mat4,
    exposure: f32,
}

// Runs at each view transition: Before shadows, before reflections, etc..
fn prepare_view(
    mut commands: Commands,
    phase: If<Res<RenderPhase>>,
    camera: Single<(
        Entity,
        &Camera,
        &GlobalTransform,
        &Projection,
        Option<&Exposure>,
    )>,
    shadow: Option<Res<DirectionalLightInfo>>,
    reflect: Option<Single<&ReflectionPlane>>,
) {
    let (camera_entity, _camera, cam_global_trans, cam_proj, exposure) = *camera;

    let view_position;
    let mut world_from_view;
    let view_from_world;
    let clip_from_world;

    if **phase == RenderPhase::Shadow {
        if let Some(shadow) = &shadow {
            view_position = shadow.cascade.world_from_cascade.project_point3(Vec3::ZERO);
            //clip_from_view = shadow.cascade.clip_from_cascade;
            world_from_view = shadow.cascade.world_from_cascade;
            view_from_world = world_from_view.inverse();
            clip_from_world = shadow.cascade.clip_from_world;
        } else {
            return;
        }
    } else {
        view_position = cam_global_trans.translation();
        let clip_from_view = cam_proj.get_clip_from_view();
        world_from_view = cam_global_trans.to_matrix();
        if let Some(reflect) = reflect
            && phase.reflection()
        {
            world_from_view = reflect.0 * world_from_view;
        }
        view_from_world = world_from_view.inverse();
        clip_from_world = clip_from_view * view_from_world;
    }

    commands.entity(camera_entity).insert(ViewUniforms {
        position: view_position,
        world_from_view,
        from_world: view_from_world,
        clip_from_world,
        exposure: exposure
            .map(|e| e.exposure())
            .unwrap_or_else(|| Exposure::default().exposure()),
    });
}

// It seems like some drivers are limited by code length.
// The point light loop is unrolled so setting this too high can be an issue.
const MAX_POINT_LIGHTS: usize = 8;

fn render_std_mat(
    mesh_entities: Query<(
        Entity,
        &ViewVisibility,
        &GlobalTransform,
        &Mesh3d,
        &Aabb,
        &MeshMaterial3d<StandardMaterial>,
        Has<SkipReflection>,
        Has<ReadReflection>,
    )>,
    camera: Single<(&ViewUniforms, Option<&EnvironmentMapLight>)>,
    point_lights: Query<(&PointLight, &GlobalTransform)>,
    spot_lights: Query<(&SpotLight, &GlobalTransform)>,
    mut ctx: NonSendMut<BevyGlContext>,
    mut gpu_meshes: NonSendMut<GPUMeshBufferMap>,
    materials: Res<Assets<StandardMaterial>>,
    phase: If<Res<RenderPhase>>,
    mut transparent_draws: ResMut<DeferredAlphaBlendDraws>,
    shadow: Option<Res<DirectionalLightInfo>>,
    gpu_images: NonSend<GpuImages>,
    bevy_window: Single<&Window>,
    directional_lights: Query<(&Transform, &DirectionalLight)>,
    reflect_texture: Option<Res<PlaneReflectionTexture>>,
    mut plane_reflection: Option<Single<(&mut ReflectionPlane, &GlobalTransform)>>,
    args: Res<Args>,
) {
    let (view, env_light) = *camera;
    let phase = **phase;

    let shadow_def;

    if phase.depth_only() {
        shadow_def = shadow
            .as_ref()
            .map_or(("", ""), |_| ("RENDER_DEPTH_ONLY", ""));
    } else {
        shadow_def = shadow.as_ref().map_or(("", ""), |_| ("SAMPLE_SHADOW", ""));
    }

    let max_point_lights = format!("{MAX_POINT_LIGHTS}");
    let max_lights_def = ("MAX_POINT_LIGHTS", max_point_lights.as_str());

    let shader_index = if args.npr {
        shader_cached!(
            ctx,
            "../assets/shaders/std_mat.vert",
            "../assets/shaders/npr_std_mat.frag",
            &[shadow_def, max_lights_def]
        )
        .unwrap()
    } else {
        shader_cached!(
            ctx,
            "../assets/shaders/std_mat.vert",
            "../assets/shaders/pbr_std_mat.frag",
            &[shadow_def, max_lights_def]
        )
        .unwrap()
    };

    gpu_meshes.reset_bind_cache();
    ctx.use_cached_program(shader_index);

    let mut build = UniformSlotBuilder::<StandardMaterial>::new(&ctx, &gpu_images, shader_index);

    queue_val!(build, double_sided);
    queue_tex!(build, base_color_texture);
    queue_val!(build, base_color);

    build.load("world_from_view", view.world_from_view);
    build.load("view_position", view.position);
    build.load("clip_from_world", view.clip_from_world);

    let view_resolution = vec2(
        bevy_window.physical_width() as f32,
        bevy_window.physical_height() as f32,
    );
    load_val!(build, view_resolution);
    build.load("write_reflection", phase.reflection());
    let mut reflect_bool_location = None;

    if !phase.depth_only() {
        queue_val!(build, flip_normal_map_y);
        queue_val!(build, perceptual_roughness);
        queue_val!(build, metallic);
        queue_val!(build, emissive);
        build.queue_val("reflectance", |m| {
            m.specular_tint.to_linear().to_vec3() * m.reflectance
        });
        queue_tex!(build, normal_map_texture);
        queue_tex!(build, metallic_roughness_texture);
        queue_tex!(build, emissive_texture);

        let env_light = env_light.unwrap();

        let specular_map = env_light.specular_map.clone();
        load_tex!(build, specular_map);
        let diffuse_map = env_light.diffuse_map.clone();
        load_tex!(build, diffuse_map);
        build.load("env_intensity", env_light.intensity);

        if let Some(shadow) = &shadow {
            let shadow_texture = shadow.texture.clone();
            load_tex!(build, shadow_texture);
            let shadow_clip_from_world = shadow.cascade.clip_from_world;
            load_val!(build, shadow_clip_from_world);
        }

        if let Some((trans, light)) = directional_lights.iter().next() {
            build.load("directional_light_dir", trans.forward().as_vec3());
            build.load(
                "directional_light_color",
                light.color.to_linear().to_vec3() * light.illuminance,
            );
        } else {
            build.load("directional_light_dir", Vec3::ZERO);
            build.load("directional_light_color", Vec3::ZERO);
        }

        build.queue_val("alpha_blend", |m| {
            transparent_draw_from_alpha_mode(&m.alpha_mode)
        });
        build.queue_val("has_normal_map", |m| m.normal_map_texture.is_some());
        build.load("view_exposure", view.exposure);

        let mut point_light_position_range = Vec::new();
        let mut point_light_color_radius = Vec::new();
        let mut spot_light_dir_offset_scale = Vec::new();

        for (light, trans) in &point_lights {
            if spot_light_dir_offset_scale.len() >= MAX_POINT_LIGHTS {
                break;
            }
            point_light_position_range.push(trans.translation().extend(light.range));
            point_light_color_radius.push(
                (light.color.to_linear().to_vec3() * light.intensity * POWER_TO_INTENSITY)
                    .extend(light.radius),
            );
            spot_light_dir_offset_scale.push(vec4(1.0, 0.0, 2.0, 1.0));
        }

        for (light, trans) in &spot_lights {
            if spot_light_dir_offset_scale.len() >= MAX_POINT_LIGHTS {
                break;
            }
            point_light_position_range.push(trans.translation().extend(light.range));
            point_light_color_radius.push(
                (light.color.to_linear().to_vec3() * light.intensity * POWER_TO_INTENSITY)
                    .extend(light.radius),
            );
            spot_light_dir_offset_scale.push(spot_dir_offset_scale(light, trans));
        }

        let light_count = point_light_position_range.len() as i32;
        load_val!(build, light_count);
        load_slice!(build, point_light_position_range);
        load_slice!(build, point_light_color_radius);
        load_slice!(build, spot_light_dir_offset_scale);

        reflect_bool_location = build.get_uniform_location("read_reflection");
        if let Some(reflect_texture) = &reflect_texture {
            if reflect_bool_location.is_some() {
                let reflect_texture = reflect_texture.texture.clone();
                load_tex!(build, reflect_texture);
            }
        }

        if let Some(plane) = &mut plane_reflection {
            build.load("reflection_plane_position", plane.1.translation());
            build.load("reflection_plane_normal", plane.1.up().as_vec3());
        }
    }

    let iter = if phase.transparent() {
        Either::Right(mesh_entities.iter_many(transparent_draws.take()))
    } else {
        Either::Left(mesh_entities.iter())
    };

    for (entity, view_vis, transform, mesh, aabb, material_h, skip_reflect, read_reflect) in iter {
        if phase.can_use_camera_frustum_cull() && !view_vis.get() {
            continue;
        }

        if skip_reflect && phase.reflection() {
            continue;
        }

        let Some(material) = materials.get(material_h) else {
            continue;
        };

        let world_from_local = transform.to_matrix();

        if !phase.depth_only() {
            // If in opaque phase we must defer any alpha blend draws so they can be sorted and run in order.
            if !transparent_draws.maybe_defer::<StandardMaterial>(
                transparent_draw_from_alpha_mode(&material.alpha_mode),
                phase,
                entity,
                transform,
                aabb,
                &view.from_world,
                &world_from_local,
            ) {
                continue;
            }
            reflect_bool_location
                .clone()
                .map(|loc| (read_reflect && phase.read_reflect()).load(&ctx, &loc));
            set_blend_func_from_alpha_mode(&ctx.gl, &material.alpha_mode);
        }

        load_val!(build, world_from_local);

        build.run(material);
        gpu_meshes.draw_mesh(&ctx, mesh.id(), shader_index);
    }
}

fn spot_dir_offset_scale(light: &SpotLight, trans: &GlobalTransform) -> Vec4 {
    // https://github.com/bevyengine/bevy/blob/abb8c353f49a6fe9e039e82adbe1040488ad910a/crates/bevy_pbr/src/render/light.rs#L846
    let cos_outer = light.outer_angle.cos();
    let spot_scale = 1.0 / (light.inner_angle.cos() - cos_outer).max(1e-4);
    let spot_offset = -cos_outer * spot_scale;
    octahedral_encode(trans.forward().as_vec3())
        .extend(spot_offset)
        .extend(spot_scale)
}

// Map from luminous power in lumens to luminous intensity in lumens per steradian for a point light.
// For details see: https://google.github.io/filament/Filament.html#mjx-eqn-pointLightLuminousPower
const POWER_TO_INTENSITY: f32 = 1.0 / (4.0 * PI);

use std::f32::consts::PI;

use argh::FromArgs;
use bevy::{
    camera::{Exposure, primitives::Aabb},
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    core_pipeline::{prepass::DepthPrepass, tonemapping::Tonemapping},
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    light::{
        CascadeShadowConfigBuilder, TransmittedShadowReceiver, light_consts::lux::DIRECT_SUNLIGHT,
    },
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    scene::SceneInstanceReady,
    window::PresentMode,
    winit::WinitSettings,
};
use bevy_mod_mipmap_generator::{MipmapGeneratorPlugin, generate_mipmaps};
use bevy_opengl::{
    BevyGlContext, UniformValue,
    bevy_standard_lighting::{
        DEFAULT_MAX_JOINTS_DEF, DEFAULT_MAX_LIGHTS_DEF, bind_standard_lighting, standard_pbr_glsl,
        standard_pbr_lighting_glsl, standard_shadow_sampling_glsl,
    },
    load_tex, load_val,
    phase_shadow::DirectionalLightShadow,
    phase_transparent::DeferredAlphaBlendDraws,
    plane_reflect::{PlaneReflectionTexture, ReflectionPlane},
    prepare_image::GpuImages,
    prepare_joints::JointData,
    prepare_mesh::GPUMeshBufferMap,
    queue_tex, queue_val,
    render::{
        OpenGLRenderPlugins, RenderPhase, RenderSet, register_prepare_system,
        register_render_system, set_blend_func_from_alpha_mode, transparent_draw_from_alpha_mode,
    },
    shader_cached,
    uniform_slot_builder::UniformSlotBuilder,
};
use itertools::{Either, Itertools};

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

const FOX_PATH: &str = "models/animated/Fox.glb";

#[derive(Component)]
struct AnimationToPlay {
    graph_handle: Handle<AnimationGraph>,
    index: AnimationNodeIndex,
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut ctx: Option<NonSendMut<BevyGlContext>>,
    mut _meshes: ResMut<Assets<Mesh>>,
    mut _materials: ResMut<Assets<StandardMaterial>>,
    mut _graphs: ResMut<Assets<AnimationGraph>>,
) {
    if let Some(ctx) = &mut ctx {
        ctx.add_snippet("agx", include_str!("../assets/shaders/agx.glsl"));
        ctx.add_snippet(
            "std_mat_bindings",
            include_str!("../assets/shaders/std_mat_bindings.glsl"),
        );
        ctx.add_snippet("math", include_str!("../assets/shaders/math.glsl"));
        ctx.add_snippet("shadow_sampling", standard_shadow_sampling_glsl());
        ctx.add_snippet("pbr", standard_pbr_glsl());
        ctx.add_snippet("standard_pbr_lighting", standard_pbr_lighting_glsl());
    }

    // --------------------------
    //let (graph, index) = AnimationGraph::from_clip(
    //    asset_server.load(GltfAssetLabel::Animation(2).from_asset(FOX_PATH)),
    //);
    //let graph_handle = graphs.add(graph);
    //let animation_to_play = AnimationToPlay {
    //    graph_handle,
    //    index,
    //};
    //let mesh_scene = SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(FOX_PATH)));
    //commands
    //    .spawn((
    //        animation_to_play,
    //        mesh_scene,
    //        Transform::from_scale(Vec3::ONE * 0.01),
    //    ))
    //    .observe(play_animation_when_ready);
    // --------------------------

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

    //commands
    //    .spawn((
    //        SceneRoot(
    //            asset_server.load("models/bistro/bistro_exterior/BistroExterior.gltf#Scene0"),
    //        ),
    //        Transform::from_xyz(0.0, 0.0, 0.0),
    //    ))
    //    .observe(proc_scene);
    //commands
    //    .spawn((
    //        SceneRoot(
    //            asset_server
    //                .load("models/bistro/bistro_interior_wine/BistroInterior_Wine.gltf#Scene0"),
    //        ),
    //        Transform::from_xyz(0.0, 0.0, 0.0),
    //    ))
    //    .observe(proc_scene);

    //commands
    //    .spawn((
    //        SceneRoot(asset_server.load("models/caldera/hotel_01.glb#Scene0")),
    //        Transform::from_scale(Vec3::ONE * 0.01),
    //    ))
    //    .observe(proc_scene);

    //commands.spawn((
    //    SceneRoot(
    //        asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/DamagedHelmet.glb")),
    //    ),
    //    Transform::from_scale(Vec3::ONE * 5.0).with_translation(vec3(0.0, 5.0, 0.0)),
    //));

    // Reflection plane
    //commands.spawn((
    //    Mesh3d(meshes.add(Plane3d::default().mesh().size(500.0, 500.0))),
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

fn play_animation_when_ready(
    scene_ready: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    animations_to_play: Query<&AnimationToPlay>,
    mut players: Query<&mut AnimationPlayer>,
) {
    if let Ok(animation_to_play) = animations_to_play.get(scene_ready.entity) {
        for child in children.iter_descendants(scene_ready.entity) {
            if let Ok(mut player) = players.get_mut(child) {
                player.play(animation_to_play.index).repeat();
                commands
                    .entity(child)
                    .insert(AnimationGraphHandle(animation_to_play.graph_handle.clone()));
            }
        }
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
                match mat.alpha_mode {
                    AlphaMode::Mask(_) => {
                        mat.diffuse_transmission = 0.6;
                        mat.double_sided = true;
                        mat.cull_mode = None;
                        commands.entity(entity).insert(TransmittedShadowReceiver);
                    }
                    _ => (),
                }
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
    shadow: Option<Res<DirectionalLightShadow>>,
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
        Option<&JointData>,
    )>,
    camera: Single<(&ViewUniforms, Option<&EnvironmentMapLight>)>,
    point_lights: Query<(&PointLight, &GlobalTransform)>,
    spot_lights: Query<(&SpotLight, &GlobalTransform)>,
    directional_lights: Query<(&DirectionalLight, &GlobalTransform)>,
    mut ctx: NonSendMut<BevyGlContext>,
    mut gpu_meshes: NonSendMut<GPUMeshBufferMap>,
    materials: Res<Assets<StandardMaterial>>,
    phase: If<Res<RenderPhase>>,
    mut transparent_draws: ResMut<DeferredAlphaBlendDraws>,
    shadow: Option<Res<DirectionalLightShadow>>,
    gpu_images: NonSend<GpuImages>,
    bevy_window: Single<&Window>,
    reflect_texture: Option<Res<PlaneReflectionTexture>>,
    mut plane_reflection: Option<Single<(&mut ReflectionPlane, &GlobalTransform)>>,
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

    let shader_index = shader_cached!(
        ctx,
        "../assets/shaders/std_mat.vert",
        "../assets/shaders/pbr_std_mat.frag",
        &[shadow_def, DEFAULT_MAX_LIGHTS_DEF, DEFAULT_MAX_JOINTS_DEF]
    )
    .unwrap();

    gpu_meshes.reset_bind_cache();
    ctx.use_cached_program(shader_index);

    let mut build = UniformSlotBuilder::<StandardMaterial>::new(&ctx, &gpu_images, shader_index);

    queue_val!(build, double_sided);
    queue_tex!(build, base_color_texture);
    queue_val!(build, base_color);

    build.load("world_from_view", view.world_from_view);
    build.load("view_position", view.position);
    build.load("clip_from_world", view.clip_from_world);
    build.load("view_exposure", view.exposure);

    let view_resolution = vec2(
        bevy_window.physical_width() as f32,
        bevy_window.physical_height() as f32,
    );
    load_val!(build, view_resolution);
    build.load("write_reflection", phase.reflection());
    let mut reflect_bool_location = None;

    if !phase.depth_only() {
        queue_val!(build, emissive);
        queue_val!(build, metallic);
        queue_val!(build, perceptual_roughness);
        queue_val!(build, diffuse_transmission);
        queue_val!(build, flip_normal_map_y);
        build.queue_val("reflectance", |m| {
            m.specular_tint.to_linear().to_vec3() * m.reflectance
        });
        queue_tex!(build, normal_map_texture);
        queue_tex!(build, metallic_roughness_texture);
        queue_tex!(build, emissive_texture);

        build.queue_val("alpha_blend", |m| {
            transparent_draw_from_alpha_mode(&m.alpha_mode)
        });
        build.queue_val("has_normal_map", |m| m.normal_map_texture.is_some());

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

        bind_standard_lighting(
            &mut build,
            point_lights.iter(),
            spot_lights.iter(),
            directional_lights.single().ok(),
            env_light,
            shadow.as_deref(),
        );
    }

    let iter = if phase.transparent() {
        Either::Right(mesh_entities.iter_many(transparent_draws.take()))
    } else {
        // Sort by material. Can be faster if enough entities share materials (faster on bistro and san-miguel).
        // TODO avoid re-sorting?
        Either::Left(
            mesh_entities
                .iter()
                .sorted_by_key(|(_, _, _, _, _, material_h, _, _, _)| material_h.id()),
        )
        // Either::Left(mesh_entities.iter()) // <- Unsorted alternative
    };

    let mut last_material = None;
    for (
        entity,
        view_vis,
        transform,
        mesh,
        aabb,
        material_h,
        skip_reflect,
        read_reflect,
        joint_data,
    ) in iter
    {
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

        if let Some(joint_data) = joint_data {
            build.load("joint_data", joint_data.as_slice());
        }
        build.load("has_joint_data", joint_data.is_some());

        // Only re-bind if the material has changed.
        if last_material != Some(material_h) {
            build.run(material);
        }
        gpu_meshes.draw_mesh(&ctx, mesh.id(), shader_index);
        last_material = Some(material_h);
    }
}

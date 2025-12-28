use std::f32::consts::PI;

use bevy::{
    camera::primitives::Aabb,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    light::{CascadeShadowConfigBuilder, light_consts::lux},
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    window::PresentMode,
    winit::{UpdateMode, WinitSettings},
};
use bevy_basic_camera::{CameraController, CameraControllerPlugin};
use bevy_mod_mipmap_generator::{MipmapGeneratorPlugin, generate_mipmaps};
use bevy_opengl::{
    BevyGlContext,
    prepare_image::GpuImages,
    prepare_mesh::GPUMeshBufferMap,
    render::{
        DeferredAlphaBlendDraws, DirectionalLightInfo, OpenGLRenderPlugin, RenderPhase,
        RenderRunner, RenderSet,
    },
    tex,
    uniform_slot_builder::{Tex, UniformSlotBuilder},
    upload, val,
};
use itertools::Either;

fn main() {
    let mut app = App::new();
    app.insert_resource(WinitSettings {
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
    ));

    {
        let world = app.world_mut();
        let render_std_mat_id = world.register_system(render_std_mat);
        world
            .get_resource_mut::<RenderRunner>()
            .unwrap()
            .register::<StandardMaterial>(render_std_mat_id);
    }

    app.add_plugins(MipmapGeneratorPlugin)
        .add_systems(Update, generate_mipmaps::<StandardMaterial>)
        .add_systems(Startup, setup.in_set(RenderSet::Pipeline))
        .run();
}

/// set up a simple 3D scene
fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut ctx: NonSendMut<BevyGlContext>,
) {
    ctx.shader_snippets
        .insert(String::from("agx"), String::from(include_str!("agx.glsl")));

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
    camera: Single<(Entity, &Camera, &GlobalTransform, &Projection)>,
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
    let (_entity, _camera, cam_global_trans, cam_proj) = *camera;
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
        bevy_opengl::shader_cached!(ctx, "npr_std_mat.vert", "npr_std_mat.frag", &[shadow_def])
            .unwrap();
    gpu_meshes.reset_bind_cache();
    ctx.use_cached_program(shader_index);

    let mut build = UniformSlotBuilder::<StandardMaterial>::new(&ctx, &gpu_images, shader_index);

    val!(build, flip_normal_map_y);
    val!(build, double_sided);
    val!(build, perceptual_roughness);
    val!(build, metallic);

    tex!(build, base_color_texture);
    tex!(build, normal_map_texture);
    tex!(build, metallic_roughness_texture);

    if let Some(shadow) = &shadow {
        let shadow_texture = shadow.texture;
        build.tex("shadow_texture", move |_| Tex::Gl(shadow_texture));
        let shadow_clip_from_world = shadow.cascade.clip_from_world;
        upload!(build, shadow_clip_from_world);
    }

    if let Some(trans) = directional_lights.iter().next() {
        build.upload("directional_light_dir_to_light", trans.back().as_vec3());
    } else {
        build.upload("directional_light_dir_to_light", Vec3::ZERO);
    }

    build.val("alpha_blend", |m| material_alpha_blend(m));
    build.val("base_color", |m| m.base_color.to_linear().to_vec4());

    upload!(build, world_from_view);
    upload!(build, view_position);

    let view_resolution = vec2(
        bevy_window.physical_width().max(1) as f32,
        bevy_window.physical_height().max(1) as f32,
    );
    upload!(build, view_resolution);

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

        upload!(build, world_from_local);
        upload!(build, clip_from_local);

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

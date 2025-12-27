use std::{any::TypeId, mem};

use bevy::{
    camera::primitives::Aabb,
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
    render::{DeferredAlphaBlendDraws, OpenGLRenderPlugin, RenderPhase, RenderRunner},
    tex,
    unifrom_slot_builder::UniformSlotBuilder,
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
        .add_systems(Startup, (setup, init))
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
    gpu_images: NonSend<GpuImages>,
) {
    let (_entity, _camera, cam_global_trans, cam_proj) = *camera;

    let view_position = cam_global_trans.translation();
    let clip_from_view = cam_proj.get_clip_from_view();
    let world_from_view = cam_global_trans.to_matrix();
    let view_from_world = world_from_view.inverse();

    let view_from_clip = clip_from_view.inverse();

    let clip_from_world = clip_from_view * view_from_world;
    let _world_from_clip = world_from_view * view_from_clip;

    let shader_index = bevy_opengl::shader_cached!(
        ctx,
        "npr_std_mat.vert",
        "npr_std_mat.frag",
        Default::default(),
        Default::default()
    )
    .unwrap();

    ctx.use_cached_program(shader_index);

    let mut build = UniformSlotBuilder::<StandardMaterial>::new(&ctx, &gpu_images, shader_index);

    val!(build, flip_normal_map_y);
    val!(build, double_sided);
    val!(build, perceptual_roughness);
    val!(build, metallic);

    tex!(build, base_color_texture);
    tex!(build, normal_map_texture);
    tex!(build, metallic_roughness_texture);

    build.val("alpha_blend", |m| material_alpha_blend(m));
    build.val("base_color", |m| m.base_color.to_linear().to_vec4());

    upload!(build, world_from_view);
    upload!(build, view_position);

    gpu_meshes.reset_bind_cache();
    ctx.use_cached_program(shader_index);

    let iter = match **phase {
        RenderPhase::Opaque => Either::Left(mesh_entities.iter()),
        RenderPhase::Transparent => {
            Either::Right(mesh_entities.iter_many(mem::take(&mut transparent_draws.next)))
        }
    };

    for (entity, view_vis, transform, mesh, aabb, material_h) in iter {
        if !view_vis.get() {
            continue;
        }

        let Some(material) = materials.get(material_h) else {
            continue;
        };
        let world_from_local = transform.to_matrix();
        let clip_from_local = clip_from_world * world_from_local;

        if **phase == RenderPhase::Opaque {
            // If in opaque phase and must defer any alpha blend draws so they can be sorted and run in order.
            if material_alpha_blend(material) {
                let ws_radius = transform.radius_vec3a(aabb.half_extents);
                let ws_center = world_from_local.transform_point3a(aabb.center);
                transparent_draws.deferred.push((
                    // Use closest point on bounding sphere
                    view_from_world.project_point3a(ws_center).z + ws_radius,
                    entity,
                    TypeId::of::<StandardMaterial>(),
                ));
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

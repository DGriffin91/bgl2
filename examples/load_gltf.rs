use std::{any::TypeId, mem};

use bevy::{
    asset::{AssetMetaCheck, UnapprovedPathMode},
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
use glow::HasContext;
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
            })
            .set(AssetPlugin {
                // Wasm builds will check for meta files (that don't exist) if this isn't set.
                // This causes errors and even panics in web builds on itch.
                // See https://github.com/bevyengine/bevy_github_ci_template/issues/48.
                meta_check: AssetMetaCheck::Never,
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
        Ref<GlobalTransform>,
        Ref<Mesh3d>,
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
    let view_to_clip = cam_proj.get_clip_from_view();
    let view_to_world = cam_global_trans.to_matrix();
    let world_to_view = view_to_world.inverse();

    let clip_to_view = view_to_clip.inverse();

    let world_to_clip = view_to_clip * world_to_view;
    let _clip_to_world = view_to_world * clip_to_view;

    #[cfg(not(any(target_arch = "wasm32", feature = "bundle_shaders")))]
    let shader_index = {
        ctx.shader_cached(
            "examples/npr_std_mat.vert",
            "examples/npr_std_mat.frag",
            Default::default(),
        )
        .unwrap()
    };

    #[cfg(any(target_arch = "wasm32", feature = "bundle_shaders"))]
    let shader_index =
        bevy_opengl::shader_cached_include!(ctx, "npr_std_mat.vert", "npr_std_mat.frag", defs)
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

    upload!(build, view_to_world);
    upload!(build, view_position);

    gpu_meshes.reset_bind_cache();
    ctx.use_cached_program(shader_index);

    let iter = match **phase {
        RenderPhase::Opaque => Either::Left(mesh_entities.iter()),
        RenderPhase::Transparent => {
            Either::Right(mesh_entities.iter_many(mem::take(&mut transparent_draws.next)))
        }
    };

    for (entity, view_vis, transform, mesh, material_h) in iter {
        if !view_vis.get() {
            continue;
        }

        let Some(material) = materials.get(material_h) else {
            continue;
        };

        if **phase == RenderPhase::Opaque {
            // If in opaque phase and must defer any alpha blend draws so they can be sorted and run in order.
            if material_alpha_blend(material) {
                transparent_draws.deferred.push((
                    world_to_view
                        .project_point3a(transform.translation_vec3a())
                        .z,
                    entity,
                    TypeId::of::<StandardMaterial>(),
                ));
                continue;
            }
        }

        let local_to_world = transform.to_matrix();
        let local_to_clip = world_to_clip * local_to_world;

        upload!(build, local_to_world);
        upload!(build, local_to_clip);

        build.run(material);
        gpu_meshes.draw_mesh(&ctx, mesh.id(), shader_index);
    }

    unsafe { ctx.gl.bind_vertex_array(None) };
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

use std::f32::consts::PI;

use bevy::{
    camera::primitives::Aabb,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    scene::SceneInstanceReady,
    window::PresentMode,
    winit::WinitSettings,
};
use bevy_mod_mipmap_generator::{MipmapGeneratorPlugin, generate_mipmaps};
use bevy_opengl::{
    UniformSet, UniformValue,
    bevy_standard_lighting::{OpenGLStandardLightingPlugin, StandardLightingUniforms},
    bevy_standard_material::{
        DrawsSortedByMaterial, ReadReflection, SkipReflection, StandardMaterialUniforms,
        ViewUniforms, init_std_shader_includes, sort_std_mat_by_material,
        standard_material_prepare_view,
    },
    command_encoder::CommandEncoder,
    flip_cull_mode,
    phase_shadow::DirectionalLightShadow,
    phase_transparent::DeferredAlphaBlendDraws,
    plane_reflect::{ReflectionPlane, ReflectionUniforms},
    prepare_image::GpuImages,
    prepare_joints::JointData,
    prepare_mesh::GpuMeshes,
    render::{
        OpenGLRenderPlugins, RenderPhase, RenderSet, register_prepare_system,
        set_blend_func_from_alpha_mode, transparent_draw_from_alpha_mode,
    },
    shader_cached,
};
use bevy_opengl::{
    bevy_standard_lighting::{DEFAULT_MAX_JOINTS_DEF, DEFAULT_MAX_LIGHTS_DEF},
    render::register_render_system,
};
use itertools::Either;
use uniform_set_derive::UniformSet;

fn main() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.32, 0.4, 0.47)))
        .insert_resource(WinitSettings::continuous())
        .insert_resource(GlobalAmbientLight::NONE)
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
            FreeCameraPlugin,
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
            MipmapGeneratorPlugin,
        ));

    app.init_resource::<DrawsSortedByMaterial>()
        .add_plugins((OpenGLRenderPlugins, OpenGLStandardLightingPlugin))
        .add_systems(Update, sort_std_mat_by_material.in_set(RenderSet::Prepare))
        .add_systems(
            Startup,
            init_std_shader_includes.in_set(RenderSet::Pipeline),
        );

    register_prepare_system(app.world_mut(), standard_material_prepare_view);
    register_render_system::<StandardMaterial, _>(app.world_mut(), standard_material_render);
    register_render_system::<HazeMaterial, _>(app.world_mut(), render_haze_mat);

    app.add_systems(Update, generate_mipmaps::<StandardMaterial>)
        .add_systems(Startup, setup.in_set(RenderSet::Pipeline))
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-45.0, 4.0, 0.0).looking_at(Vec3::new(0.0, 18.0, 0.0), Vec3::Y),
        EnvironmentMapLight {
            diffuse_map: asset_server.load("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
            specular_map: asset_server.load("environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
            intensity: 10.0,
            ..default()
        },
        FreeCamera {
            walk_speed: 10.0,
            run_speed: 30.0,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: PI / 3.0,
            ..default()
        }),
    ));

    commands.spawn(SceneRoot(
        asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/arena/arena.gltf")),
    ));

    commands
        .spawn(SceneRoot(asset_server.load(
            GltfAssetLabel::Scene(0).from_asset("models/arena/red_pools_upper.gltf"),
        )))
        .observe(proc_pools_upper);
    commands
        .spawn(SceneRoot(asset_server.load(
            GltfAssetLabel::Scene(0).from_asset("models/arena/red_pools_lower.gltf"),
        )))
        .observe(proc_pools_lower);
    commands
        .spawn(SceneRoot(asset_server.load(
            GltfAssetLabel::Scene(0).from_asset("models/arena/red_pools_entrance.gltf"),
        )))
        .observe(proc_pools_entrance);
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(100.0, 100.0))),
        Transform::from_translation(vec3(0.0, -30.0, 0.0)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::BLACK,
            ..default()
        })),
    ));

    commands.insert_resource(LightMap {
        light_map: asset_server.load("models/arena/bake4ke.hdr"),
    });

    commands
        .spawn(SceneRoot(asset_server.load(
            GltfAssetLabel::Scene(0).from_asset("models/arena/haze_sun.gltf"),
        )))
        .observe(remove_std_mat)
        .observe(
            |ready: On<SceneInstanceReady>, mut commands: Commands, children: Query<&Children>| {
                let m = HazeMaterial::spawn(&mut commands, vec4(1.0, 0.7, 0.5, 0.9));
                decend_haze(ready.entity, &mut commands, children, m);
            },
        );

    let red_haze1 = HazeMaterial::spawn(&mut commands, vec4(1.0, 0.0, 0.0, 0.6));
    let red_haze1_ob =
        move |ready: On<SceneInstanceReady>, mut commands: Commands, children: Query<&Children>| {
            decend_haze(ready.entity, &mut commands, children, red_haze1);
        };
    commands
        .spawn(SceneRoot(asset_server.load(
            GltfAssetLabel::Scene(0).from_asset("models/arena/haze_nave.gltf"),
        )))
        .observe(remove_std_mat)
        .observe(red_haze1_ob);

    commands
        .spawn(SceneRoot(asset_server.load(
            GltfAssetLabel::Scene(0).from_asset("models/arena/haze_sanct_side.gltf"),
        )))
        .observe(remove_std_mat)
        .observe(red_haze1_ob);

    commands
        .spawn(SceneRoot(asset_server.load(
            GltfAssetLabel::Scene(0).from_asset("models/arena/haze_chan.gltf"),
        )))
        .observe(remove_std_mat)
        .observe(
            |ready: On<SceneInstanceReady>, mut commands: Commands, children: Query<&Children>| {
                let m = HazeMaterial::spawn(&mut commands, vec4(0.3, 0.0, 0.0, 0.5));
                decend_haze(ready.entity, &mut commands, children, m);
            },
        );

    commands
        .spawn(SceneRoot(asset_server.load(
            GltfAssetLabel::Scene(0).from_asset("models/arena/haze_font.gltf"),
        )))
        .observe(remove_std_mat)
        .observe(
            |ready: On<SceneInstanceReady>, mut commands: Commands, children: Query<&Children>| {
                let m = HazeMaterial::spawn(&mut commands, vec4(0.4, 0.05, 1.0, 0.5));
                decend_haze(ready.entity, &mut commands, children, m);
            },
        );

    // Reflection plane
    commands.spawn((
        Transform::from_translation(vec3(0.0, 2.6, 0.0)),
        ReflectionPlane::default(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
        Transform::from_translation(vec3(-12.0, -0.1, 0.0)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::linear_rgba(0.0, 0.0, 0.0, 0.8),
            perceptual_roughness: 0.1,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        SkipReflection,
        ReadReflection,
    ));
}

pub fn proc_pools_upper(
    scene_ready: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    has_std_mat: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in children.iter_descendants(scene_ready.entity) {
        if let Ok(mat_h) = has_std_mat.get(entity) {
            if let Some(mat) = materials.get_mut(mat_h) {
                mat.emissive = LinearRgba::rgb(0.1, 0.0, 0.0);
                mat.base_color = Color::BLACK;
                mat.alpha_mode = AlphaMode::Opaque;
                mat.perceptual_roughness = 0.0;
            }
            commands
                .entity(entity)
                .insert((SkipReflection, ReadReflection));
        }
    }
}

pub fn proc_pools_lower(
    scene_ready: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    has_std_mat: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in children.iter_descendants(scene_ready.entity) {
        if let Ok(mat_h) = has_std_mat.get(entity) {
            if let Some(mat) = materials.get_mut(mat_h) {
                mat.emissive = LinearRgba::rgb(0.1, 0.0, 0.0);
                mat.base_color = Color::BLACK;
                mat.alpha_mode = AlphaMode::Opaque;
                mat.perceptual_roughness = 0.0;
            }
            commands
                .entity(entity)
                .insert((SkipReflection, ReadReflection));
        }
    }
}

pub fn proc_pools_entrance(
    scene_ready: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    has_std_mat: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in children.iter_descendants(scene_ready.entity) {
        if let Ok(mat_h) = has_std_mat.get(entity) {
            if let Some(mat) = materials.get_mut(mat_h) {
                mat.base_color = Color::linear_rgba(0.5, 0.0, 0.0, 0.8);
                mat.emissive = LinearRgba::rgb(0.1, 0.0, 0.0);
                mat.alpha_mode = AlphaMode::Blend;
                mat.perceptual_roughness = 0.3;
                mat.lightmap_exposure = 0.0;
            }
            commands
                .entity(entity)
                .insert((SkipReflection, ReadReflection));
        }
    }
}

pub fn remove_std_mat(
    scene_ready: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
) {
    for entity in children.iter_descendants(scene_ready.entity) {
        commands
            .entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>();
    }
}

#[derive(UniformSet, Resource, Clone)]
#[uniform_set(prefix = "ub_")]
pub struct LightMap {
    pub light_map: Handle<Image>,
}

pub fn standard_material_render(
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
    view_uniforms: Single<&ViewUniforms>,
    materials: Res<Assets<StandardMaterial>>,
    phase: Res<RenderPhase>,
    light_map: Res<LightMap>,
    mut transparent_draws: ResMut<DeferredAlphaBlendDraws>,
    reflect_uniforms: Option<Res<ReflectionUniforms>>,
    sorted: Res<DrawsSortedByMaterial>,
    mut enc: ResMut<CommandEncoder>,
    shadow: Option<Res<DirectionalLightShadow>>,
) {
    let view_uniforms = view_uniforms.clone();

    let phase = *phase;

    let iter = if phase.transparent() {
        Either::Right(mesh_entities.iter_many(transparent_draws.take()))
    } else {
        Either::Left(mesh_entities.iter_many(&**sorted))
    };

    struct Draw {
        world_from_local: Mat4,
        joint_data: Option<JointData>,
        material_h: AssetId<StandardMaterial>,
        material_idx: u32,
        read_reflect: bool,
        mesh: Handle<Mesh>,
    }

    let mut draws = Vec::new();
    let mut render_materials: Vec<StandardMaterialUniforms> = Vec::new();

    let mut last_material = None;
    let mut current_material_idx = 0;
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
        if (phase.can_use_camera_frustum_cull() && !view_vis.get())
            || (skip_reflect && phase.reflection())
        {
            continue;
        }

        let Some(material) = materials.get(material_h) else {
            continue;
        };

        let world_from_local = transform.to_matrix();

        // If in opaque phase we must defer any alpha blend draws so they can be sorted and run in order.
        if !transparent_draws.maybe_defer::<StandardMaterial>(
            transparent_draw_from_alpha_mode(&material.alpha_mode),
            phase,
            entity,
            transform,
            aabb,
            &view_uniforms.view_from_world,
            &world_from_local,
        ) {
            continue;
        }

        if last_material != Some(material_h) {
            current_material_idx = render_materials.len() as u32;
            last_material = Some(material_h);
            render_materials.push(material.into());
        }

        draws.push(Draw {
            // TODO don't copy full material
            material_idx: current_material_idx,
            world_from_local,
            joint_data: joint_data.cloned(),
            material_h: material_h.id(),
            read_reflect,
            mesh: mesh.0.clone(),
        });
    }

    let reflect_uniforms = reflect_uniforms.as_deref().cloned();

    let shadow = shadow.as_deref().cloned();
    let light_map = light_map.clone();
    enc.record(move |ctx, world| {
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
            "../assets/shaders/arena_mat.vert",
            "../assets/shaders/arena_mat.frag",
            &[shadow_def, DEFAULT_MAX_LIGHTS_DEF, DEFAULT_MAX_JOINTS_DEF],
            &[
                ViewUniforms::bindings(),
                StandardMaterialUniforms::bindings(),
                StandardLightingUniforms::bindings(),
                LightMap::bindings(),
            ]
        )
        .unwrap();

        world.resource_mut::<GpuMeshes>().reset_mesh_bind_cache();
        ctx.use_cached_program(shader_index);

        ctx.load("write_reflection", phase.reflection());

        ctx.map_uniform_set_locations::<ViewUniforms>();
        ctx.map_uniform_set_locations::<LightMap>();
        ctx.map_uniform_set_locations::<StandardMaterialUniforms>();
        ctx.bind_uniforms_set(
            world.resource::<GpuImages>(),
            world.resource::<ViewUniforms>(),
        );
        ctx.bind_uniforms_set(world.resource::<GpuImages>(), &light_map);

        let mut reflect_bool_location = None;
        if !phase.depth_only() {
            ctx.map_uniform_set_locations::<StandardLightingUniforms>();
            ctx.bind_uniforms_set(
                world.resource::<GpuImages>(),
                world.resource::<StandardLightingUniforms>(),
            );

            reflect_bool_location = ctx.get_uniform_location("read_reflection");
            ctx.map_uniform_set_locations::<ReflectionUniforms>();
            ctx.bind_uniforms_set(
                world.resource::<GpuImages>(),
                reflect_uniforms.as_ref().unwrap_or(&Default::default()),
            );
        }

        let mut last_material = None;
        for draw in &draws {
            let material = &render_materials[draw.material_idx as usize];
            set_blend_func_from_alpha_mode(&ctx.gl, &material.alpha_mode);

            ctx.load("world_from_local", draw.world_from_local);

            if let Some(joint_data) = &draw.joint_data {
                ctx.load("joint_data", joint_data.as_slice());
            }
            ctx.load("has_joint_data", draw.joint_data.is_some());

            reflect_bool_location.clone().map(|loc| {
                (draw.read_reflect && phase.read_reflect() && reflect_uniforms.is_some())
                    .load(&ctx.gl, &loc)
            });

            // Only re-bind if the material has changed.
            if last_material != Some(draw.material_h) {
                ctx.set_cull_mode(flip_cull_mode(material.cull_mode, phase.reflection()));
                ctx.bind_uniforms_set(world.resource::<GpuImages>(), material);
            }

            world
                .resource_mut::<GpuMeshes>()
                .draw_mesh(ctx, draw.mesh.id(), shader_index);
            last_material = Some(draw.material_h);
        }
    });
}

// -----------------------------------------------------
// -----------------------------------------------------
// -----------------------------------------------------

#[derive(Clone, Component, UniformSet)]
#[uniform_set(prefix = "ub_")]
struct HazeMaterial {
    haze_color: Vec4,
}

impl HazeMaterial {
    pub fn spawn(commands: &mut Commands, color: Vec4) -> Entity {
        commands.spawn(HazeMaterial { haze_color: color }).id()
    }
}

fn decend_haze(
    scene_ready: Entity,
    commands: &mut Commands,
    children: Query<&Children>,
    haze_material: Entity,
) {
    for entity in children.iter_descendants(scene_ready) {
        commands.entity(entity).insert(HazeHandle(haze_material));
    }
}

#[derive(Component, Deref, DerefMut)]
struct HazeHandle(Entity);

fn render_haze_mat(
    mesh_entities: Query<(
        Entity,
        &ViewVisibility,
        &GlobalTransform,
        &Aabb,
        &Mesh3d,
        &HazeHandle,
    )>,
    materials: Query<&HazeMaterial>,
    phase: If<Res<RenderPhase>>,
    mut enc: ResMut<CommandEncoder>,
    shadow: Option<Res<DirectionalLightShadow>>,
    mut transparent_draws: ResMut<DeferredAlphaBlendDraws>,
    view_uniforms: Single<&ViewUniforms>,
) {
    let phase = **phase;
    if !(phase.defer_transparent() || phase.transparent()) {
        return;
    }

    let mut draws = Vec::new();

    struct DrawData {
        world_from_local: Mat4,
        material: HazeMaterial,
        mesh: AssetId<Mesh>,
    }

    let iter = if phase.transparent() {
        Either::Right(mesh_entities.iter_many(transparent_draws.take()))
    } else {
        Either::Left(mesh_entities.iter())
    };

    for (entity, view_vis, transform, aabb, mesh, material_h) in iter {
        if !view_vis.get() {
            continue;
        }
        let Ok(material) = materials.get(**material_h) else {
            continue;
        };

        let world_from_local = transform.to_matrix();
        if phase.defer_transparent() {
            let ws_radius = transform.radius_vec3a(aabb.half_extents);
            let ws_center = world_from_local.transform_point3a(aabb.center);
            transparent_draws.defer::<HazeMaterial>(
                // Use closest point on bounding sphere
                view_uniforms.view_from_world.project_point3a(ws_center).z + ws_radius,
                entity,
            );
        } else if phase.transparent() {
            draws.push(DrawData {
                world_from_local,
                material: material.clone(),
                mesh: mesh.id(),
            });
        }
    }

    if !phase.transparent() {
        return;
    }
    let shadow = shadow.as_deref().cloned();

    enc.record(move |ctx, world| {
        let shadow_def = if phase.depth_only() {
            shadow
                .as_ref()
                .map_or(("", ""), |_| ("RENDER_DEPTH_ONLY", ""))
        } else {
            shadow.as_ref().map_or(("", ""), |_| ("SAMPLE_SHADOW", ""))
        };

        let shader_index = bevy_opengl::shader_cached!(
            ctx,
            "../assets/shaders/haze_material.vert",
            "../assets/shaders/haze_material.frag",
            &[shadow_def, DEFAULT_MAX_LIGHTS_DEF],
            &[
                ViewUniforms::bindings(),
                StandardLightingUniforms::bindings(),
                HazeMaterial::bindings()
            ]
        )
        .unwrap();

        world.resource_mut::<GpuMeshes>().reset_mesh_bind_cache();
        ctx.use_cached_program(shader_index);

        ctx.map_uniform_set_locations::<HazeMaterial>();
        ctx.map_uniform_set_locations::<ViewUniforms>();

        ctx.bind_uniforms_set(
            world.resource::<GpuImages>(),
            world.resource::<ViewUniforms>(),
        );
        if !phase.depth_only() {
            ctx.map_uniform_set_locations::<StandardLightingUniforms>();
            ctx.bind_uniforms_set(
                world.resource::<GpuImages>(),
                world.resource::<StandardLightingUniforms>(),
            );
        }
        ctx.set_cull_mode(None);

        for draw in &draws {
            ctx.load("ub_world_from_local", draw.world_from_local);
            ctx.bind_uniforms_set(world.resource::<GpuImages>(), &draw.material);
            world
                .resource_mut::<GpuMeshes>()
                .draw_mesh(ctx, draw.mesh, shader_index);
        }
    });
}

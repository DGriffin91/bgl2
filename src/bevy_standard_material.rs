use bevy::{
    camera::{Exposure, primitives::Aabb},
    prelude::*,
};
use itertools::{Either, Itertools};
use uniform_set_derive::UniformSet;
use wgpu_types::Face;

use crate::{
    UniformSet, UniformValue,
    bevy_standard_lighting::{
        DEFAULT_MAX_JOINTS_DEF, DEFAULT_MAX_LIGHTS_DEF, StandardLightingUniforms,
        standard_pbr_glsl, standard_pbr_lighting_glsl, standard_shadow_sampling_glsl,
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
        RenderPhase, RenderSet, register_prepare_system, register_render_system,
        set_blend_func_from_alpha_mode, transparent_draw_from_alpha_mode,
    },
    shader_cached,
};

#[derive(Resource, Clone, Default)]
pub struct OpenGLStandardMaterialSettings {
    pub no_point: bool, // no point light glsl code
}

#[derive(Default)]
pub struct OpenGLStandardMaterialPlugin;

impl Plugin for OpenGLStandardMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DrawsSortedByMaterial>();
        app.init_resource::<OpenGLStandardMaterialSettings>();
        register_prepare_system(app.world_mut(), standard_material_prepare_view);
        register_render_system::<StandardMaterial, _>(app.world_mut(), standard_material_render);
        app.add_systems(
            Startup,
            init_std_shader_includes.in_set(RenderSet::Pipeline),
        );
        app.add_systems(Update, sort_std_mat_by_material.in_set(RenderSet::Prepare));
    }
}

pub fn init_std_shader_includes(mut enc: ResMut<CommandEncoder>) {
    enc.record(|ctx, _world| {
        ctx.add_shader_include("std::agx", include_str!("shaders/agx.glsl"));
        ctx.add_shader_include("std::math", include_str!("shaders/math.glsl"));
        ctx.add_shader_include("std::shadow_sampling", standard_shadow_sampling_glsl());
        ctx.add_shader_include("std::pbr", standard_pbr_glsl());
        ctx.add_shader_include("std::pbr_lighting", standard_pbr_lighting_glsl());
    });
}

#[derive(Component, Default)]
pub struct SkipReflection;

#[derive(Component, Default)]
pub struct ReadReflection;

#[derive(UniformSet, Component, Resource, Clone)]
#[uniform_set(prefix = "ub_")]
pub struct ViewUniforms {
    pub world_from_view: Mat4,
    pub view_from_world: Mat4,
    pub clip_from_world: Mat4,
    pub view_position: Vec3,
    pub view_resolution: Vec2,
    pub view_exposure: f32,
}

#[derive(Resource, Default, Deref, DerefMut)]
pub struct DrawsSortedByMaterial(Vec<Entity>);

pub fn sort_std_mat_by_material(
    mesh_entities: Query<(Entity, &MeshMaterial3d<StandardMaterial>)>,
    mut sorted: ResMut<DrawsSortedByMaterial>,
) {
    sorted.clear();
    for (entity, _) in mesh_entities
        .iter()
        .sorted_by_key(|(_, material_h)| material_h.id())
    {
        sorted.push(entity);
    }
}

// Runs at each view transition: Before shadows, before reflections, etc..
pub fn standard_material_prepare_view(
    mut commands: Commands,
    phase: Res<RenderPhase>,
    camera: Single<(
        Entity,
        &Camera,
        &GlobalTransform,
        &Projection,
        Option<&Exposure>,
    )>,
    shadow: Option<Res<DirectionalLightShadow>>,
    reflect: Option<Single<&ReflectionPlane>>,
    bevy_window: Single<&Window>,
    mut enc: ResMut<CommandEncoder>,
) {
    let (camera_entity, _camera, cam_global_trans, cam_proj, exposure) = *camera;
    let view_resolution = vec2(
        bevy_window.physical_width() as f32,
        bevy_window.physical_height() as f32,
    );

    let view_position;
    let mut world_from_view;
    let view_from_world;
    let clip_from_world;

    if *phase == RenderPhase::Shadow {
        if let Some(shadow) = &shadow {
            view_position = shadow.light_position;
            view_from_world = shadow.view_from_world;
            world_from_view = shadow.view_from_world.inverse();
            clip_from_world = shadow.clip_from_view * shadow.view_from_world;
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

    let view_uniforms = ViewUniforms {
        world_from_view,
        view_from_world,
        clip_from_world,
        view_position,
        view_resolution,
        view_exposure: exposure
            .map(|e| e.exposure())
            .unwrap_or_else(|| Exposure::default().exposure()),
    };
    commands.entity(camera_entity).insert(view_uniforms.clone());
    enc.record(move |_ctx, world| {
        world.insert_resource(view_uniforms.clone());
    });
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
    mut transparent_draws: ResMut<DeferredAlphaBlendDraws>,
    reflect_uniforms: Option<Res<ReflectionUniforms>>,
    sorted: Res<DrawsSortedByMaterial>,
    mut enc: ResMut<CommandEncoder>,
    prefs: Res<OpenGLStandardMaterialSettings>,
    shadow: Option<Res<DirectionalLightShadow>>,
) {
    let view_uniforms = view_uniforms.clone();

    let phase = *phase;

    let iter = if phase.transparent() {
        Either::Right(mesh_entities.iter_many(transparent_draws.take()))
    } else {
        Either::Left(mesh_entities.iter_many(&**sorted))
        // Either::Left(mesh_entities.iter()) // <- Unsorted alternative
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
    let prefs = prefs.clone();
    let shadow = shadow.as_deref().cloned();
    enc.record(move |ctx, world| {
        let shader_index = shader_cached!(
            ctx,
            "shaders/std_mat.vert",
            "shaders/pbr_std_mat.frag",
            [DEFAULT_MAX_LIGHTS_DEF, DEFAULT_MAX_JOINTS_DEF]
                .iter()
                .chain(
                    world
                        .resource::<StandardLightingUniforms>()
                        .shader_defs(!prefs.no_point, shadow.is_some(), &phase)
                        .iter()
                ),
            &[
                ViewUniforms::bindings(),
                StandardMaterialUniforms::bindings(),
                StandardLightingUniforms::bindings()
            ]
        )
        .unwrap();

        world.resource_mut::<GpuMeshes>().reset_mesh_bind_cache();
        ctx.use_cached_program(shader_index);

        ctx.load("write_reflection", phase.reflection());

        ctx.map_uniform_set_locations::<ViewUniforms>();
        ctx.map_uniform_set_locations::<StandardMaterialUniforms>();
        ctx.bind_uniforms_set(
            world.resource::<GpuImages>(),
            world.resource::<ViewUniforms>(),
        );

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

#[derive(UniformSet, Component, Clone)]
#[uniform_set(prefix = "ub_")]
pub struct StandardMaterialUniforms {
    pub base_color: Vec4,
    pub emissive: Vec4,
    pub perceptual_roughness: f32,
    pub metallic: f32,
    pub double_sided: bool,
    pub diffuse_transmission: f32,
    pub lightmap_exposure: f32,
    pub flip_normal_map_y: bool,
    pub reflectance: Vec3,
    pub alpha_blend: bool,
    pub has_normal_map: bool,
    pub base_color_texture: Option<Handle<Image>>,
    pub normal_map_texture: Option<Handle<Image>>,
    pub metallic_roughness_texture: Option<Handle<Image>>,
    pub emissive_texture: Option<Handle<Image>>,
    #[exclude]
    pub alpha_mode: AlphaMode,
    #[exclude]
    pub cull_mode: Option<Face>,
}

impl From<&StandardMaterial> for StandardMaterialUniforms {
    fn from(mat: &StandardMaterial) -> Self {
        Self {
            base_color: mat.base_color.to_linear().to_vec4(),
            emissive: mat.emissive.to_vec4(),
            perceptual_roughness: mat.perceptual_roughness,
            metallic: mat.metallic,
            double_sided: mat.double_sided,
            diffuse_transmission: mat.diffuse_transmission,
            lightmap_exposure: mat.lightmap_exposure,
            flip_normal_map_y: mat.flip_normal_map_y,
            reflectance: mat.specular_tint.to_linear().to_vec3() * mat.reflectance,
            alpha_blend: transparent_draw_from_alpha_mode(&mat.alpha_mode),
            has_normal_map: mat.normal_map_texture.is_some(),
            base_color_texture: mat.base_color_texture.clone(),
            normal_map_texture: mat.normal_map_texture.clone(),
            metallic_roughness_texture: mat.metallic_roughness_texture.clone(),
            emissive_texture: mat.emissive_texture.clone(),
            alpha_mode: mat.alpha_mode,
            cull_mode: mat.cull_mode,
        }
    }
}

use bevy::{
    camera::{Exposure, primitives::Aabb},
    prelude::*,
};
use itertools::{Either, Itertools};
use uniform_set_derive::UniformSet;

use crate::{
    SlotData, UniformSet, UniformValue,
    bevy_standard_lighting::{
        DEFAULT_MAX_JOINTS_DEF, DEFAULT_MAX_LIGHTS_DEF, bind_standard_lighting, standard_pbr_glsl,
        standard_pbr_lighting_glsl, standard_shadow_sampling_glsl,
    },
    clone2,
    command_encoder::CommandEncoder,
    faststack::StackStack,
    flip_cull_mode, load_if_new, load_match, load_tex_if_new,
    phase_shadow::DirectionalLightShadow,
    phase_transparent::DeferredAlphaBlendDraws,
    plane_reflect::{ReflectionPlane, ReflectionUniforms},
    prepare_image::GpuImages,
    prepare_joints::JointData,
    render::{
        RenderPhase, RenderSet, register_prepare_system, register_render_system,
        set_blend_func_from_alpha_mode, transparent_draw_from_alpha_mode,
    },
    shader_cached,
};

#[derive(Default)]
pub struct OpenGLStandardMaterialPlugin;

impl Plugin for OpenGLStandardMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DrawsSortedByMaterial>();
        register_prepare_system(app.world_mut(), standard_material_prepare_view);
        register_render_system::<StandardMaterial, _>(app.world_mut(), standard_material_render);
        app.add_systems(Startup, setup.in_set(RenderSet::Pipeline));
        app.add_systems(Update, sort_by_material.in_set(RenderSet::Prepare));
    }
}

fn setup(mut cmd: ResMut<CommandEncoder>) {
    cmd.record(|ctx| {
        ctx.add_snippet("std::agx", include_str!("shaders/agx.glsl"));
        ctx.add_snippet("std::math", include_str!("shaders/math.glsl"));
        ctx.add_snippet("std::shadow_sampling", standard_shadow_sampling_glsl());
        ctx.add_snippet("std::pbr", standard_pbr_glsl());
        ctx.add_snippet("std::pbr_lighting", standard_pbr_lighting_glsl());
    });
}

#[derive(Component, Default)]
pub struct SkipReflection;

#[derive(Component, Default)]
pub struct ReadReflection;

#[derive(UniformSet, Component, Clone)]
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

pub fn sort_by_material(
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
        world_from_view,
        view_from_world,
        clip_from_world,
        view_position,
        view_resolution,
        view_exposure: exposure
            .map(|e| e.exposure())
            .unwrap_or_else(|| Exposure::default().exposure()),
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
    camera: Single<(&ViewUniforms, Option<&EnvironmentMapLight>)>,
    point_lights: Query<(&PointLight, &GlobalTransform)>,
    spot_lights: Query<(&SpotLight, &GlobalTransform)>,
    directional_lights: Query<(&DirectionalLight, &GlobalTransform)>,
    materials: Res<Assets<StandardMaterial>>,
    phase: Res<RenderPhase>,
    mut transparent_draws: ResMut<DeferredAlphaBlendDraws>,
    shadow: Option<Res<DirectionalLightShadow>>,
    reflect_uniforms: Option<Res<ReflectionUniforms>>,
    sorted: Res<DrawsSortedByMaterial>,
    mut cmd: ResMut<CommandEncoder>,
) {
    let (view_uniforms, env_light) = *camera;
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
        material: StandardMaterial,
        read_reflect: bool,
        mesh: Handle<Mesh>,
    }

    let mut draws = Vec::new();

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

        draws.push(Draw {
            // TODO don't copy full material
            material: material.clone(),
            world_from_local,
            joint_data: joint_data.cloned(),
            material_h: material_h.id(),
            read_reflect,
            mesh: mesh.0.clone(),
        });
    }

    let point_lights = point_lights
        .iter()
        .map(|(a, b)| (a.clone(), b.clone()))
        .collect::<Vec<_>>();
    let spot_lights = spot_lights
        .iter()
        .map(|(a, b)| (a.clone(), b.clone()))
        .collect::<Vec<_>>();
    let directional_light = clone2(directional_lights.single().ok());
    let env_light = env_light.cloned();

    let view_uniforms = view_uniforms.clone();
    let shadow = shadow.as_deref().cloned();
    let reflect_uniforms = reflect_uniforms.as_deref().cloned();
    cmd.record(move |ctx| {
        let env_light = env_light.clone();
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
            "shaders/std_mat.vert",
            "shaders/pbr_std_mat.frag",
            &[shadow_def, DEFAULT_MAX_LIGHTS_DEF, DEFAULT_MAX_JOINTS_DEF]
        )
        .unwrap();

        ctx.reset_mesh_bind_cache();
        ctx.use_cached_program(shader_index);

        ctx.load("write_reflection", phase.reflection());

        ctx.map_uniform_set_locations::<ViewUniforms>();
        ctx.bind_uniforms_set(&view_uniforms);
        ctx.map_uniform_set_locations::<StandardMaterial>();
        let mut reflect_bool_location = None;

        if !phase.depth_only() {
            reflect_bool_location = ctx.get_uniform_location("read_reflection");
            if let Some(reflect_uniforms) = &reflect_uniforms {
                ctx.map_uniform_set_locations::<ReflectionUniforms>();
                ctx.bind_uniforms_set(reflect_uniforms);
            }

            let point_lights = point_lights.iter().map(|(a, b)| (a, b));
            let spot_lights = spot_lights.iter().map(|(a, b)| (a, b));

            bind_standard_lighting(
                ctx,
                point_lights,
                spot_lights,
                directional_light,
                env_light,
                shadow.as_ref(),
            );
        }

        let mut last_material = None;
        for draw in &draws {
            set_blend_func_from_alpha_mode(&ctx.gl, &draw.material.alpha_mode);

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
                ctx.set_cull_mode(flip_cull_mode(draw.material.cull_mode, phase.reflection()));
                ctx.bind_uniforms_set(&draw.material);
            }

            ctx.draw_mesh(draw.mesh.id(), shader_index);
            last_material = Some(draw.material_h);
        }
    });
}

// Implementing this manually for bevy's standard material, see #[derive(UniformSet)] for typical uses.
impl UniformSet for StandardMaterial {
    fn names() -> &'static [(&'static str, bool)] {
        &[
            ("base_color", false),
            ("emissive", false),
            ("perceptual_roughness", false),
            ("metallic", false),
            ("double_sided", false),
            ("diffuse_transmission", false),
            ("flip_normal_map_y", false),
            ("reflectance", false),
            ("alpha_blend", false),
            ("has_normal_map", false),
            ("base_color_texture", true),
            ("normal_map_texture", true),
            ("metallic_roughness_texture", true),
            ("emissive_texture", true),
        ]
    }

    fn load(
        &self,
        gl: &glow::Context,
        gpu_images: &GpuImages,
        index: u32,
        slot: &mut SlotData,
        temp: &mut StackStack<u32, 16>,
    ) {
        load_match!(index, gl, gpu_images, slot, temp, {
            0 => value(self.base_color),
            1 => value(self.emissive),
            2 => value(self.perceptual_roughness),
            3 => value(self.metallic),
            4 => value(self.double_sided),
            5 => value(self.diffuse_transmission),
            6 => value(self.flip_normal_map_y),
            7 => value(self.specular_tint.to_linear().to_vec3() * self.reflectance),
            8 => value(transparent_draw_from_alpha_mode(&self.alpha_mode)),
            9 => value(self.normal_map_texture.is_some()),
            10 => tex(self.base_color_texture.clone().into()),
            11 => tex(self.normal_map_texture.clone().into()),
            12 => tex(self.metallic_roughness_texture.clone().into()),
            13 => tex(self.emissive_texture.clone().into()),
        });
    }
}

use bevy::{
    camera::{Exposure, primitives::Aabb},
    prelude::*,
};
use itertools::{Either, Itertools};

use crate::{
    BevyGlContext, SlotData, Tex, UniformSet, UniformValue,
    bevy_standard_lighting::{
        DEFAULT_MAX_JOINTS_DEF, DEFAULT_MAX_LIGHTS_DEF, bind_standard_lighting, standard_pbr_glsl,
        standard_pbr_lighting_glsl, standard_shadow_sampling_glsl,
    },
    faststack::StackStack,
    load_if_new, load_tex_if_new,
    phase_shadow::DirectionalLightShadow,
    phase_transparent::DeferredAlphaBlendDraws,
    plane_reflect::{PlaneReflectionTexture, ReflectionPlane},
    prepare_image::GpuImages,
    prepare_joints::JointData,
    prepare_mesh::GPUMeshBufferMap,
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
        register_prepare_system(app.world_mut(), standard_material_prepare_view);
        register_render_system::<StandardMaterial, _>(app.world_mut(), standard_material_render);
        app.add_systems(Startup, setup.in_set(RenderSet::Pipeline));
    }
}

fn setup(mut ctx: Option<NonSendMut<BevyGlContext>>) {
    if let Some(ctx) = &mut ctx {
        ctx.add_snippet("std::agx", include_str!("shaders/agx.glsl"));
        ctx.add_snippet("std::math", include_str!("shaders/math.glsl"));
        ctx.add_snippet("std::shadow_sampling", standard_shadow_sampling_glsl());
        ctx.add_snippet("std::pbr", standard_pbr_glsl());
        ctx.add_snippet("std::pbr_lighting", standard_pbr_lighting_glsl());
    }
}

#[derive(Component, Default)]
pub struct SkipReflection;

#[derive(Component, Default)]
pub struct ReadReflection;

#[derive(Component, Clone)]
pub struct ViewUniforms {
    pub position: Vec3,
    pub world_from_view: Mat4,
    pub from_world: Mat4,
    pub clip_from_world: Mat4,
    pub exposure: f32,
}

// Runs at each view transition: Before shadows, before reflections, etc..
pub fn standard_material_prepare_view(
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
        "shaders/std_mat.vert",
        "shaders/pbr_std_mat.frag",
        &[shadow_def, DEFAULT_MAX_LIGHTS_DEF, DEFAULT_MAX_JOINTS_DEF]
    )
    .unwrap();

    gpu_meshes.reset_bind_cache();
    ctx.use_cached_program(shader_index);

    ctx.load("world_from_view", view.world_from_view);
    ctx.load("view_position", view.position);
    ctx.load("clip_from_world", view.clip_from_world);
    ctx.load("view_exposure", view.exposure);

    let view_resolution = vec2(
        bevy_window.physical_width() as f32,
        bevy_window.physical_height() as f32,
    );
    ctx.load("view_resolution", view_resolution);
    ctx.load("write_reflection", phase.reflection());
    let mut reflect_bool_location = None;

    ctx.map_uniform_set_locations::<StandardMaterial>();

    if !phase.depth_only() {
        reflect_bool_location = ctx.get_uniform_location("read_reflection");
        if let Some(reflect_texture) = &reflect_texture {
            if reflect_bool_location.is_some() {
                let reflect_texture = reflect_texture.texture.clone();
                ctx.load_tex("reflect_texture", &Tex::Gl(reflect_texture), &gpu_images);
            }
        }

        if let Some(plane) = &mut plane_reflection {
            ctx.load("reflection_plane_position", plane.1.translation());
            ctx.load("reflection_plane_normal", plane.1.up().as_vec3());
        }

        bind_standard_lighting(
            &mut ctx,
            &gpu_images,
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
                .map(|loc| (read_reflect && phase.read_reflect()).load(&ctx.gl, &loc));
            set_blend_func_from_alpha_mode(&ctx.gl, &material.alpha_mode);
        }

        ctx.load("world_from_local", world_from_local);

        if let Some(joint_data) = joint_data {
            ctx.load("joint_data", joint_data.as_slice());
        }
        ctx.load("has_joint_data", joint_data.is_some());

        // Only re-bind if the material has changed.
        if last_material != Some(material_h) {
            ctx.set_cull_mode(material.cull_mode);
            ctx.bind_uniforms_set(&gpu_images, material);
        }

        gpu_meshes.draw_mesh(&ctx, mesh.id(), shader_index);
        last_material = Some(material_h);
    }
}

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
        match index {
            0 => load_if_new(&self.base_color, gl, slot, temp),
            1 => {
                load_if_new(&self.emissive, gl, slot, temp);
            }
            2 => {
                load_if_new(&self.perceptual_roughness, gl, slot, temp);
            }
            3 => {
                load_if_new(&self.metallic, gl, slot, temp);
            }
            4 => {
                load_if_new(&self.double_sided, gl, slot, temp);
            }
            5 => {
                load_if_new(&self.diffuse_transmission, gl, slot, temp);
            }
            6 => {
                load_if_new(&self.flip_normal_map_y, gl, slot, temp);
            }
            7 => {
                let reflectance = self.specular_tint.to_linear().to_vec3() * self.reflectance;
                load_if_new(&reflectance, gl, slot, temp);
            }
            8 => {
                load_if_new(
                    &transparent_draw_from_alpha_mode(&self.alpha_mode),
                    gl,
                    slot,
                    temp,
                );
            }
            9 => {
                load_if_new(&self.normal_map_texture.is_some(), gl, slot, temp);
            }
            10 => {
                load_tex_if_new(
                    &self.base_color_texture.clone().into(),
                    gl,
                    gpu_images,
                    slot,
                );
            }
            11 => {
                load_tex_if_new(
                    &self.normal_map_texture.clone().into(),
                    gl,
                    gpu_images,
                    slot,
                );
            }
            12 => {
                load_tex_if_new(
                    &self.metallic_roughness_texture.clone().into(),
                    gl,
                    gpu_images,
                    slot,
                );
            }
            13 => {
                load_tex_if_new(&self.emissive_texture.clone().into(), gl, gpu_images, slot);
            }
            _ => unreachable!(),
        }
    }
}

use std::{f32::consts::PI, ops::Deref};

use bevy::prelude::*;
use uniform_set_derive::UniformSet;

use crate::{
    clone2,
    command_encoder::CommandEncoder,
    mesh_util::octahedral_encode,
    phase_shadow::DirectionalLightShadow,
    prepare_image::TextureRef,
    render::{RenderPhase, RenderSet},
};

// It seems like some drivers are limited by code length.
// The point light loop is unrolled so setting this too high can be an issue.
// Also fragment shader uniform capacity can be very limited on some drivers.
pub const DEFAULT_MAX_POINT_LIGHTS: usize = 8;
pub const DEFAULT_MAX_LIGHTS_DEF: (&str, &str) = ("MAX_POINT_LIGHTS", "8");

// vertex shader uniform capacity can be limited on some drivers (though not as much as in the frag shader.)
pub const DEFAULT_MAX_JOINTS: usize = 32;
pub const DEFAULT_MAX_JOINTS_DEF: (&str, &str) = ("MAX_JOINTS", "32");

#[derive(UniformSet, Resource, Clone, Default)]
#[uniform_set(prefix = "ub_")]
pub struct StandardLightingUniforms {
    #[array_max("MAX_POINT_LIGHTS")]
    pub point_light_position_range: Vec<Vec4>,
    #[array_max("MAX_POINT_LIGHTS")]
    pub point_light_color_radius: Vec<Vec4>,
    #[array_max("MAX_POINT_LIGHTS")]
    pub spot_light_dir_offset_scale: Vec<Vec4>,
    pub directional_light_dir: Vec3,
    pub directional_light_color: Vec3,
    #[base_type("samplerCube")]
    pub specular_map: Option<Handle<Image>>,
    #[base_type("samplerCube")]
    pub diffuse_map: Option<Handle<Image>>,
    pub shadow_texture: TextureRef,
    pub env_intensity: f32,
    pub shadow_clip_from_world: Mat4,
    pub light_count: i32,
}

impl StandardLightingUniforms {
    pub fn shader_defs(
        &self,
        point: bool,
        shadow: bool,
        phase: &RenderPhase,
    ) -> [(&'static str, &'static str); 3] {
        [
            if !point || self.light_count == 0 {
                ("NO_POINT", "")
            } else {
                ("", "")
            },
            if self.specular_map.is_some() && self.diffuse_map.is_some() {
                ("", "")
            } else {
                ("NO_ENV", "")
            },
            if phase.depth_only() && shadow {
                ("RENDER_DEPTH_ONLY", "")
            } else {
                if shadow {
                    ("SAMPLE_SHADOW", "")
                } else {
                    ("", "")
                }
            },
        ]
    }
}

#[derive(Default)]
pub struct OpenGLStandardLightingPlugin;

impl Plugin for OpenGLStandardLightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StandardLightingUniforms>()
            .add_systems(Update, prepare_standard_lighting.in_set(RenderSet::Prepare));
    }
}

fn prepare_standard_lighting(
    point_lights: Query<(&PointLight, &GlobalTransform)>,
    spot_lights: Query<(&SpotLight, &GlobalTransform)>,
    directional_lights: Query<(&DirectionalLight, &GlobalTransform)>,
    shadow: Option<Res<DirectionalLightShadow>>,
    env_light: Single<Option<&EnvironmentMapLight>, With<Camera3d>>,
    mut enc: ResMut<CommandEncoder>,
) {
    let lighting_uniform = StandardLightingUniforms::new(
        point_lights,
        spot_lights,
        clone2(directional_lights.single().ok()),
        *env_light.deref(),
        shadow.as_deref(),
        DEFAULT_MAX_POINT_LIGHTS,
    );
    enc.record(move |_ctx, world| {
        world.insert_resource(lighting_uniform);
    });
}

/// Expects SAMPLE_SHADOW shader def based on shadow availability
pub fn standard_pbr_lighting_glsl() -> &'static str {
    include_str!("shaders/standard_pbr_lighting.glsl")
}

pub fn standard_pbr_glsl() -> &'static str {
    include_str!("shaders/pbr.glsl")
}

pub fn standard_shadow_sampling_glsl() -> &'static str {
    include_str!("shaders/shadow_sampling.glsl")
}

impl StandardLightingUniforms {
    pub fn new<'a, PI, SI>(
        point_lights: PI,
        spot_lights: SI,
        directional_light: Option<(DirectionalLight, GlobalTransform)>,
        env_light: Option<&EnvironmentMapLight>,
        shadow: Option<&DirectionalLightShadow>,
        max_point_spot: usize,
    ) -> Self
    where
        PI: IntoIterator<Item = (&'a PointLight, &'a GlobalTransform)>,
        SI: IntoIterator<Item = (&'a SpotLight, &'a GlobalTransform)>,
    {
        let mut data = StandardLightingUniforms::default();

        for (light, trans) in point_lights {
            if data.point_light_position_range.len() >= max_point_spot {
                break;
            }
            data.point_light_position_range
                .push(trans.translation().extend(light.range));
            data.point_light_color_radius.push(
                (light.color.to_linear().to_vec3() * light.intensity * POWER_TO_INTENSITY)
                    .extend(light.radius),
            );
            data.spot_light_dir_offset_scale
                .push(vec4(1.0, 0.0, 2.0, 1.0));
        }

        for (light, trans) in spot_lights {
            if data.point_light_position_range.len() >= max_point_spot {
                break;
            }
            data.point_light_position_range
                .push(trans.translation().extend(light.range));
            data.point_light_color_radius.push(
                (light.color.to_linear().to_vec3() * light.intensity * POWER_TO_INTENSITY)
                    .extend(light.radius),
            );
            data.spot_light_dir_offset_scale
                .push(calc_spot_dir_offset_scale(light, trans));
        }

        data.light_count = data.point_light_position_range.len() as i32;

        if let Some((light, trans)) = directional_light {
            data.directional_light_dir = trans.forward().as_vec3();
            data.directional_light_color = light.color.to_linear().to_vec3() * light.illuminance;
        }

        if let Some(env_light) = env_light {
            data.specular_map = Some(env_light.specular_map.clone());
            data.diffuse_map = Some(env_light.diffuse_map.clone());
            data.env_intensity = env_light.intensity;
        }

        if let Some(shadow) = &shadow {
            data.shadow_texture = shadow.texture.clone();
            data.shadow_clip_from_world = shadow.clip_from_view * shadow.view_from_world;
        }

        data
    }
}

pub fn calc_spot_dir_offset_scale(light: &SpotLight, trans: &GlobalTransform) -> Vec4 {
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

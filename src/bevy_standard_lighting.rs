use std::f32::consts::PI;

use bevy::prelude::*;

use crate::{
    mesh_util::octahedral_encode, phase_shadow::DirectionalLightShadow,
    uniform_slot_builder::UniformSlotBuilder,
};

// It seems like some drivers are limited by code length.
// The point light loop is unrolled so setting this too high can be an issue.
// Also fragment shader uniform capacity can be very limited on some drivers.
pub const DEFAULT_MAX_POINT_LIGHTS: usize = 8;
pub const DEFAULT_MAX_LIGHTS_DEF: (&str, &str) = ("MAX_POINT_LIGHTS", "8");

// vertex shader uniform capacity can be limited on some drivers (though not as much as in the frag shader.)
pub const DEFAULT_MAX_JOINTS: usize = 32;
pub const DEFAULT_MAX_JOINTS_DEF: (&str, &str) = ("MAX_JOINTS", "32");

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

pub fn bind_standard_lighting<'a, T, PI, SI>(
    // UniformSlotBuilder is only needed here to maintain texture slot consistency.
    // Consider moving UniformSlotBuilder functionality into BevyGlContext and taking that instead.
    build: &mut UniformSlotBuilder<T>,
    point_lights: PI,
    spot_lights: SI,
    directional_light: Option<(&'a DirectionalLight, &'a GlobalTransform)>,
    env_light: Option<&EnvironmentMapLight>,
    shadow: Option<&DirectionalLightShadow>,
) where
    PI: IntoIterator<Item = (&'a PointLight, &'a GlobalTransform)>,
    SI: IntoIterator<Item = (&'a SpotLight, &'a GlobalTransform)>,
{
    let env_light = env_light.unwrap();

    let specular_map = env_light.specular_map.clone();
    build.queue_tex("B_specular_map", move |_| specular_map.clone().into());
    let diffuse_map = env_light.diffuse_map.clone();
    build.queue_tex("B_diffuse_map", move |_| diffuse_map.clone().into());
    build.load("B_env_intensity", env_light.intensity);

    if let Some(shadow) = &shadow {
        let shadow_texture = shadow.texture.clone();
        build.queue_tex("B_shadow_texture", move |_| shadow_texture.clone().into());
        let shadow_clip_from_world = shadow.cascade.clip_from_world;
        build.load("B_shadow_clip_from_world", shadow_clip_from_world);
    }

    if let Some((light, trans)) = directional_light {
        build.load("B_directional_light_dir", trans.forward().as_vec3());
        build.load(
            "B_directional_light_color",
            light.color.to_linear().to_vec3() * light.illuminance,
        );
    } else {
        build.load("B_directional_light_dir", Vec3::ZERO);
        build.load("B_directional_light_color", Vec3::ZERO);
    }

    let mut point_light_position_range = Vec::new();
    let mut point_light_color_radius = Vec::new();
    let mut spot_light_dir_offset_scale = Vec::new();
    for (light, trans) in point_lights {
        point_light_position_range.push(trans.translation().extend(light.range));
        point_light_color_radius.push(
            (light.color.to_linear().to_vec3() * light.intensity * POWER_TO_INTENSITY)
                .extend(light.radius),
        );
        spot_light_dir_offset_scale.push(vec4(1.0, 0.0, 2.0, 1.0));
    }

    for (light, trans) in spot_lights {
        point_light_position_range.push(trans.translation().extend(light.range));
        point_light_color_radius.push(
            (light.color.to_linear().to_vec3() * light.intensity * POWER_TO_INTENSITY)
                .extend(light.radius),
        );
        spot_light_dir_offset_scale.push(spot_dir_offset_scale(light, trans));
    }

    let light_count = point_light_position_range.len() as i32;
    build.load("B_light_count", light_count);
    build.load(
        "B_point_light_position_range",
        point_light_position_range.as_slice(),
    );
    build.load(
        "B_point_light_color_radius",
        point_light_color_radius.as_slice(),
    );
    build.load(
        "B_spot_light_dir_offset_scale",
        spot_light_dir_offset_scale.as_slice(),
    );
}

pub fn spot_dir_offset_scale(light: &SpotLight, trans: &GlobalTransform) -> Vec4 {
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

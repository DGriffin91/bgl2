use bevy::{
    light::{Cascades, cascade::Cascade},
    prelude::*,
};
use glow::{HasContext, PixelUnpackData};

use crate::{
    BevyGlContext,
    render::{RenderPhase, RenderRunner, RenderSet},
};

pub struct ShadowPhasePlugin;

impl Plugin for ShadowPhasePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, update_shadow_tex.in_set(RenderSet::Prepare));
        app.add_systems(PostUpdate, render_shadow.in_set(RenderSet::RenderShadow));
    }
}

fn update_shadow_tex(
    mut commands: Commands,
    bevy_window: Single<&Window>,
    shadow_tex: Option<Res<DirectionalLightInfo>>,
    directional_lights: Query<(&DirectionalLight, &Cascades)>,
    ctx: NonSend<BevyGlContext>,
) {
    // Keep shadow texture size up to date.

    let mut shadow_cascade = None;
    if let Some((directional_light, cascades)) = directional_lights.iter().next() {
        if directional_light.shadows_enabled {
            if let Some((_, cascades)) = cascades.cascades.iter().next() {
                if let Some(cascade) = cascades.get(0) {
                    shadow_cascade = Some(cascade.clone());
                } else {
                    commands.remove_resource::<DirectionalLightInfo>();
                    return;
                }
            }
        }
    }
    let width = bevy_window.physical_width().max(1);
    let height = bevy_window.physical_height().max(1);
    if let Some(shadow_tex) = shadow_tex {
        if let Some(shadow_cascade) = shadow_cascade {
            if shadow_tex.width != width || shadow_tex.height != height {
                unsafe {
                    ctx.gl.delete_texture(shadow_tex.texture);
                    commands.insert_resource(DirectionalLightInfo::new(
                        &ctx.gl,
                        shadow_cascade,
                        width,
                        height,
                    ))
                };
            }
        } else {
            unsafe { ctx.gl.delete_texture(shadow_tex.texture) };
            commands.remove_resource::<DirectionalLightInfo>();
        }
    } else {
        if let Some(shadow_cascade) = shadow_cascade {
            commands.insert_resource(DirectionalLightInfo::new(
                &ctx.gl,
                shadow_cascade,
                width,
                height,
            ))
        } else {
            return;
        }
    }
}

fn render_shadow(world: &mut World) {
    let Some(shadow_texture) = world.get_resource::<DirectionalLightInfo>().cloned() else {
        return;
    };
    let ctx = world.get_non_send_resource_mut::<BevyGlContext>().unwrap();
    ctx.start_opaque(true); // Reading from depth not supported so we need to write depth to color
    ctx.clear_color_and_depth();

    *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::Shadow;

    let Some(runner) = world.remove_resource::<RenderRunner>() else {
        return;
    };

    for system in &runner.prepare_registry {
        let _ = world.run_system(*system);
    }

    for (_type_id, system) in &runner.render_registry {
        let _ = world.run_system(*system);
    }

    world.insert_resource(runner);

    let ctx = world.get_non_send_resource_mut::<BevyGlContext>().unwrap();
    unsafe {
        ctx.gl
            .bind_texture(glow::TEXTURE_2D, Some(shadow_texture.texture));
        ctx.gl.copy_tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA,
            0,
            0,
            shadow_texture.width as i32,
            shadow_texture.height as i32,
            0,
        );
    };
}

#[derive(Resource, Clone)]
pub struct DirectionalLightInfo {
    pub texture: glow::Texture,
    pub cascade: Cascade,
    pub dir_to_light: Vec3,
    width: u32,
    height: u32,
}

impl DirectionalLightInfo {
    fn new(gl: &glow::Context, cascade: Cascade, width: u32, height: u32) -> Self {
        unsafe {
            let texture = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                PixelUnpackData::Slice(None),
            );
            let dir_to_light = cascade
                .world_from_cascade
                .project_point3(vec3(0.0, 0.0, 1.0));
            Self {
                texture,
                cascade,
                dir_to_light,
                width,
                height,
            }
        }
    }
}

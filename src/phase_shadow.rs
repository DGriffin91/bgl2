use bevy::{
    light::{Cascades, cascade::Cascade},
    prelude::*,
};
use glow::{HasContext, PixelUnpackData};

use crate::{
    BevyGlContext,
    command_encoder::CommandEncoder,
    prepare_image::{GpuImages, TextureRef},
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
    shadow_tex: Option<ResMut<DirectionalLightShadow>>,
    directional_lights: Query<(&DirectionalLight, &Cascades)>,
    mut enc: ResMut<CommandEncoder>,
) {
    // Keep shadow texture size up to date.
    let mut shadow_cascade = None;
    if let Some((directional_light, cascades)) = directional_lights.iter().next() {
        if directional_light.shadows_enabled {
            if let Some((_, cascades)) = cascades.cascades.iter().next() {
                if let Some(cascade) = cascades.get(0) {
                    shadow_cascade = Some(cascade.clone());
                } else {
                    commands.remove_resource::<DirectionalLightShadow>();
                    return;
                }
            }
        }
    }
    let width = bevy_window.physical_width().max(1);
    let height = bevy_window.physical_height().max(1);
    if let Some(mut shadow_tex) = shadow_tex {
        if let Some(shadow_cascade) = shadow_cascade {
            if shadow_tex.width != width || shadow_tex.height != height {
                let texture_ref = shadow_tex.texture.clone();
                shadow_tex.width = width;
                shadow_tex.height = height;
                shadow_tex.cascade = shadow_cascade;
                enc.record(move |ctx, world| unsafe {
                    if let Some((tex, _target)) = world
                        .resource_mut::<GpuImages>()
                        .texture_from_ref(&texture_ref)
                    {
                        ctx.gl.delete_texture(tex);
                        DirectionalLightShadow::init(
                            ctx,
                            &mut world.resource_mut::<GpuImages>(),
                            &texture_ref,
                            width,
                            height,
                        )
                    }
                });
            }
        } else {
            enc.delete_texture_ref(shadow_tex.texture.clone());
            commands.remove_resource::<DirectionalLightShadow>();
        }
    } else {
        if let Some(shadow_cascade) = shadow_cascade {
            let texture_ref = TextureRef::new();
            commands.insert_resource(DirectionalLightShadow {
                texture: texture_ref.clone(),
                cascade: shadow_cascade.clone(),
                dir_to_light: shadow_cascade
                    .world_from_cascade
                    .project_point3(vec3(0.0, 0.0, 1.0)),
                width,
                height,
            });
            enc.record(move |ctx, world| {
                DirectionalLightShadow::init(
                    ctx,
                    &mut world.resource_mut::<GpuImages>(),
                    &texture_ref,
                    width,
                    height,
                )
            });
        } else {
            return;
        }
    }
}

fn render_shadow(world: &mut World) {
    let Some(shadow_texture) = world.get_resource::<DirectionalLightShadow>().cloned() else {
        return;
    };
    let mut cmd = world.resource_mut::<CommandEncoder>();
    cmd.start_opaque(true); // Reading from depth not supported so we need to write depth to color
    cmd.clear_color_and_depth(None);

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

    world
        .resource_mut::<CommandEncoder>()
        .record(move |ctx, world| {
            if let Some((texture, target)) = world
                .resource_mut::<GpuImages>()
                .texture_from_ref(&shadow_texture.texture)
            {
                unsafe {
                    ctx.gl.bind_texture(target, Some(texture));
                    ctx.gl.copy_tex_image_2d(
                        target,
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
        });
}

#[derive(Resource, Clone)]
pub struct DirectionalLightShadow {
    pub texture: TextureRef,
    pub cascade: Cascade,
    pub dir_to_light: Vec3,
    width: u32,
    height: u32,
}

impl DirectionalLightShadow {
    fn init(
        ctx: &mut BevyGlContext,
        images: &mut GpuImages,
        texture_ref: &TextureRef,
        width: u32,
        height: u32,
    ) {
        unsafe {
            let texture = ctx.gl.create_texture().unwrap();
            images.add_texture_set_ref(texture, glow::TEXTURE_2D, &texture_ref);
            ctx.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            ctx.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            ctx.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            ctx.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            ctx.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            ctx.gl.tex_image_2d(
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
        }
    }
}

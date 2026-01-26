use bevy::prelude::*;
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
    directional_lights: Query<(&DirectionalLight, &GlobalTransform, Option<&ShadowBounds>)>,
    mut enc: ResMut<CommandEncoder>,
) {
    // Keep shadow texture size up to date.
    let mut view_from_world = Default::default();
    let mut clip_from_view = Default::default();
    let mut light_trans = Default::default();
    let mut enabled = false;
    if let Some((directional_light, trans, shadow_bounds)) = directional_lights.iter().next() {
        let shadow_bounds = shadow_bounds.cloned().unwrap_or_default();
        if directional_light.shadows_enabled {
            light_trans = *trans;
            let dir = light_trans
                .to_matrix()
                .transform_vector3(vec3(0.0, 0.0, -1.0));
            let position = light_trans.translation() - dir * shadow_bounds.depth * 0.5;
            let z_far = shadow_bounds.depth * 0.5;
            let shadow_view_from_world = Mat4::look_to_lh(position, dir, Vec3::Y);
            let shadow_clip_from_view = Mat4::orthographic_lh(
                -shadow_bounds.width * 0.5,
                shadow_bounds.width * 0.5,
                -shadow_bounds.height * 0.5,
                shadow_bounds.height * 0.5,
                z_far,
                0.0,
            );
            view_from_world = shadow_view_from_world;
            clip_from_view = shadow_clip_from_view;
            enabled = true;
        }
    }
    let width = bevy_window.physical_width().max(1);
    let height = bevy_window.physical_height().max(1);
    if let Some(mut shadow_tex) = shadow_tex {
        if enabled {
            shadow_tex.view_from_world = view_from_world;
            shadow_tex.clip_from_view = clip_from_view;
            shadow_tex.light_position = light_trans.translation();
            if shadow_tex.width != width || shadow_tex.height != height {
                let texture_ref = shadow_tex.texture.clone();
                shadow_tex.width = width;
                shadow_tex.height = height;

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
        if enabled {
            let texture_ref = TextureRef::new();
            commands.insert_resource(DirectionalLightShadow {
                texture: texture_ref.clone(),
                light_position: light_trans.translation(),
                view_from_world,
                clip_from_view,
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
        }
    }
}

#[derive(Component, Clone, Copy)]
pub struct ShadowBounds {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
}

impl ShadowBounds {
    pub fn cube(size: f32) -> Self {
        Self {
            width: size,
            height: size,
            depth: size,
        }
    }
}

impl Default for ShadowBounds {
    fn default() -> Self {
        Self {
            width: 50.0,
            height: 50.0,
            depth: 50.0,
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
    pub view_from_world: Mat4,
    pub clip_from_view: Mat4,
    pub light_position: Vec3,
    pub width: u32,
    pub height: u32,
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

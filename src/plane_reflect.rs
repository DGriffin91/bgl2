use bevy::prelude::*;
use glow::{HasContext, PixelUnpackData};
use uniform_set_derive::UniformSet;

use crate::{
    BevyGlContext, command_encoder::CommandEncoder, prepare_image::TextureRef, render::RenderSet,
};

pub struct PlaneReflectPlugin;

impl Plugin for PlaneReflectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, update_reflect_tex.in_set(RenderSet::Prepare));
    }
}

#[derive(UniformSet, Clone, Resource)]
pub struct ReflectionUniforms {
    reflection_plane_position: Vec3,
    reflection_plane_normal: Vec3,
    reflect_texture: TextureRef,
}

fn update_reflect_tex(
    mut commands: Commands,
    bevy_window: Single<&Window>,
    mut plane_reflection: Option<Single<(&mut ReflectionPlane, &GlobalTransform)>>,
    plane_tex: Option<Res<PlaneReflectionTexture>>,
    mut cmd: ResMut<CommandEncoder>,
) {
    // Keep reflection texture size up to date.

    let translation;
    let normal;
    if let Some(plane) = &mut plane_reflection {
        translation = plane.1.translation();
        normal = plane.1.up().as_vec3();
        **plane.0 = reflection_plane_matrix(plane.1.translation(), plane.1.up().as_vec3());
    } else {
        commands.remove_resource::<PlaneReflectionTexture>();
        commands.remove_resource::<ReflectionUniforms>();
        return;
    }
    let width = bevy_window.physical_width().max(1);
    let height = bevy_window.physical_height().max(1);

    if let Some(shadow_tex) = plane_tex {
        if plane_reflection.is_some() {
            if shadow_tex.width != width || shadow_tex.height != height {
                let texture_ref = shadow_tex.texture.clone();
                commands.insert_resource(PlaneReflectionTexture {
                    texture: texture_ref.clone(),
                    width,
                    height,
                });
                commands.insert_resource(ReflectionUniforms {
                    reflection_plane_position: translation,
                    reflection_plane_normal: normal,
                    reflect_texture: texture_ref.clone(),
                });
                cmd.record(move |ctx| {
                    unsafe {
                        if let Some((tex, _target)) = ctx.texture_from_ref(&texture_ref) {
                            ctx.gl.delete_texture(tex);
                        }
                        PlaneReflectionTexture::init(ctx, &texture_ref, width, height);
                    };
                });
            }
        } else {
            let texture_ref = shadow_tex.texture.clone();
            cmd.record(move |ctx| {
                if let Some((tex, _target)) = ctx.texture_from_ref(&texture_ref) {
                    unsafe { ctx.gl.delete_texture(tex) };
                }
            });
            commands.remove_resource::<PlaneReflectionTexture>();
            commands.remove_resource::<ReflectionUniforms>();
        }
    } else {
        if plane_reflection.is_some() {
            let texture_ref = TextureRef::new();
            commands.insert_resource(ReflectionUniforms {
                reflection_plane_position: translation,
                reflection_plane_normal: normal,
                reflect_texture: texture_ref.clone(),
            });
            commands.insert_resource(PlaneReflectionTexture {
                texture: texture_ref.clone(),
                width,
                height,
            });
            cmd.record(move |ctx| {
                PlaneReflectionTexture::init(ctx, &texture_ref, width, height);
            });
        } else {
            return;
        }
    }
}

/// Should accompany a Transform. The position and up of the transform will be used to determine the reflection plane.
#[derive(Component, Clone, Deref, DerefMut, Default)]
pub struct ReflectionPlane(pub Mat4);

#[derive(Resource, Clone)]
pub struct PlaneReflectionTexture {
    pub texture: TextureRef,
    pub width: u32,
    pub height: u32,
}

impl PlaneReflectionTexture {
    fn init(ctx: &mut BevyGlContext, texture_ref: &TextureRef, width: u32, height: u32) {
        unsafe {
            let texture = ctx.gl.create_texture().unwrap();
            ctx.add_texture_set_ref(texture, glow::TEXTURE_2D, &texture_ref);
            ctx.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            ctx.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            ctx.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
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

pub fn reflection_plane_matrix(p0: Vec3, normal: Vec3) -> Mat4 {
    let n = normal.normalize_or_zero();
    let d = -n.dot(p0);
    let r3 = Mat3::IDENTITY - 2.0 * Mat3::from_cols(n * n.x, n * n.y, n * n.z);
    let t = -2.0 * d * n;
    Mat4::from_cols(
        r3.x_axis.extend(0.0),
        r3.y_axis.extend(0.0),
        r3.z_axis.extend(0.0),
        t.extend(1.0),
    )
}

// Currently called in opaque phase
pub fn copy_reflection_texture(world: &mut World) {
    let Some(plane_reflection_texture) = world.get_resource::<PlaneReflectionTexture>().cloned()
    else {
        return;
    };
    let mut cmd = world.resource_mut::<CommandEncoder>();
    cmd.record(move |ctx| {
        unsafe {
            if let Some((tex, _target)) = ctx.texture_from_ref(&plane_reflection_texture.texture) {
                ctx.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                ctx.gl.copy_tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA,
                    0,
                    0,
                    plane_reflection_texture.width as i32,
                    plane_reflection_texture.height as i32,
                    0,
                );
            }
        };
    });
}

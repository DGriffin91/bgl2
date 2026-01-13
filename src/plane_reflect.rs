use bevy::prelude::*;
use glow::{HasContext, PixelUnpackData};

use crate::{BevyGlContext, render::RenderSet};

pub struct PlaneReflectPlugin;

impl Plugin for PlaneReflectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, update_reflect_tex.in_set(RenderSet::Prepare));
    }
}

fn update_reflect_tex(
    mut commands: Commands,
    bevy_window: Single<&Window>,
    mut plane_reflection: Option<Single<(&mut ReflectionPlane, &GlobalTransform)>>,
    shadow_tex: Option<Res<PlaneReflectionTexture>>,
    ctx: NonSend<BevyGlContext>,
) {
    // Keep reflection texture size up to date.

    if let Some(plane) = &mut plane_reflection {
        **plane.0 = reflection_plane_matrix(plane.1.translation(), plane.1.up().as_vec3());
    } else {
        commands.remove_resource::<PlaneReflectionTexture>();
        return;
    }
    let width = bevy_window.physical_width().max(1);
    let height = bevy_window.physical_height().max(1);

    if let Some(shadow_tex) = shadow_tex {
        if plane_reflection.is_some() {
            if shadow_tex.width != width || shadow_tex.height != height {
                unsafe {
                    ctx.gl.delete_texture(shadow_tex.texture);
                    commands.insert_resource(PlaneReflectionTexture::new(&ctx.gl, width, height))
                };
            }
        } else {
            unsafe { ctx.gl.delete_texture(shadow_tex.texture) };
            commands.remove_resource::<PlaneReflectionTexture>();
        }
    } else {
        if plane_reflection.is_some() {
            commands.insert_resource(PlaneReflectionTexture::new(&ctx.gl, width, height))
        } else {
            return;
        }
    }
}

/// Should accompany a Transfrom. The position and up of the transform will be used to determine the reflection plane.
#[derive(Component, Clone, Deref, DerefMut, Default)]
pub struct ReflectionPlane(pub Mat4);

#[derive(Resource, Clone)]
pub struct PlaneReflectionTexture {
    pub texture: glow::Texture,
    pub width: u32,
    pub height: u32,
}

impl PlaneReflectionTexture {
    fn new(gl: &glow::Context, width: u32, height: u32) -> Self {
        unsafe {
            let texture = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
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
            Self {
                texture,
                width,
                height,
            }
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
    let ctx = world.get_non_send_resource_mut::<BevyGlContext>().unwrap();
    unsafe {
        ctx.gl
            .bind_texture(glow::TEXTURE_2D, Some(plane_reflection_texture.texture));
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
    };
}

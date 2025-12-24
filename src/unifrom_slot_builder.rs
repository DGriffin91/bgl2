use bevy::{asset::Handle, image::Image, math::*};
use glow::HasContext;

use crate::{BevyGlContext, UniformValue, prepare_image::GpuImages};

// Probably not very fast, but writing uniforms every frame isn't either and I think the opengl uniform fn's themselves
// are maybe also dyn dispatch?

pub struct UniformSlotBuilder<'a, T> {
    pub ctx: &'a BevyGlContext,
    pub gpu_images: &'a GpuImages,
    pub shader_index: u32,

    pub value_slots: Vec<(
        glow::UniformLocation,
        Box<dyn Fn(&BevyGlContext, &T, &glow::UniformLocation)>,
    )>,

    pub texture_slots: Vec<(
        glow::UniformLocation,
        Box<dyn Fn(&T) -> &Option<Handle<Image>>>,
    )>,
}

impl<'a, T> UniformSlotBuilder<'a, T> {
    pub fn new(ctx: &'a BevyGlContext, gpu_images: &'a GpuImages, shader_index: u32) -> Self {
        UniformSlotBuilder {
            ctx,
            gpu_images,
            shader_index,
            value_slots: Vec::with_capacity(ctx.get_uniform_count(shader_index) as usize),
            texture_slots: Vec::new(),
        }
    }

    pub fn val<V, F>(&mut self, name: &str, f: F)
    where
        V: UniformValue,
        F: Fn(&T) -> V + 'static,
    {
        if let Some(location) = self.ctx.get_uniform_location(self.shader_index, name) {
            self.value_slots.push((
                location,
                Box::new(
                    move |ctx: &BevyGlContext, material: &T, loc: &glow::UniformLocation| {
                        let v: V = f(material);
                        v.upload(ctx, loc);
                    },
                ),
            ));
        }
    }

    pub fn tex<F>(&mut self, name: &str, f: F)
    where
        F: Fn(&T) -> &Option<Handle<Image>> + 'static,
    {
        if let Some(location) = self.ctx.get_uniform_location(self.shader_index, name) {
            self.texture_slots.push((location, Box::new(f)))
        }
    }
    pub fn run(&self, material: &T) {
        for (location, f) in &self.value_slots {
            f(&self.ctx, material, location)
        }
        for (i, (location, f)) in self.texture_slots.iter().enumerate() {
            let mut texture = self.gpu_images.placeholder.unwrap();
            if let Some(image_h) = f(material) {
                if let Some(t) = self.gpu_images.mapping.get(&image_h.id()) {
                    texture = *t;
                }
            }
            unsafe {
                // TODO needs to use info from the texture to actually setup correctly
                self.ctx.gl.active_texture(glow::TEXTURE0 + i as u32);
                self.ctx.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                self.ctx.gl.uniform_1_i32(Some(&location), i as i32);
            }
        }
    }

    /// Uploads immediately if location is found
    pub fn upload<V>(&self, name: &str, v: V)
    where
        V: UniformValue,
    {
        if let Some(location) = self.ctx.get_uniform_location(self.shader_index, name) {
            v.upload(&self.ctx, &location);
        }
    }
}

#[macro_export]
macro_rules! val {
    ($obj:expr, $field:ident) => {
        $obj.val(stringify!($field), |m| m.$field)
    };
}

#[macro_export]
macro_rules! tex {
    ($obj:expr, $field:ident) => {
        $obj.tex(stringify!($field), |m| &m.$field)
    };
}

#[macro_export]
macro_rules! upload {
    ($obj:expr, $field:ident) => {
        $obj.upload(stringify!($field), $field)
    };
}

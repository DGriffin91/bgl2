use bevy::{asset::Handle, image::Image, math::*, platform::collections::HashMap};
use glow::{HasContext, UniformLocation};

use crate::{BevyGlContext, UniformValue, faststack::StackStack, prepare_image::GpuImages};

// Probably not very fast, but writing uniforms every frame isn't either and I think the opengl uniform fn's themselves
// are maybe also dyn dispatch?

pub struct SlotData {
    init: bool,
    previous: StackStack<u32, 16>,
    location: glow::UniformLocation,
}

pub struct UniformSlotBuilder<'a, T> {
    pub ctx: &'a BevyGlContext,
    pub gpu_images: &'a GpuImages,
    pub shader_index: u32,

    pub value_slots: Vec<(
        SlotData,
        Box<dyn Fn(&BevyGlContext, &T, &mut SlotData, &mut StackStack<u32, 16>)>,
    )>,

    pub texture_slots: Vec<(
        glow::UniformLocation,
        Box<dyn Fn(&T) -> &Option<Handle<Image>>>,
    )>,

    pub uniform_location_cache: HashMap<String, Option<UniformLocation>>,

    pub temp_value: StackStack<u32, 16>,
}

impl<'a, T> UniformSlotBuilder<'a, T> {
    pub fn new(ctx: &'a BevyGlContext, gpu_images: &'a GpuImages, shader_index: u32) -> Self {
        UniformSlotBuilder {
            ctx,
            gpu_images,
            shader_index,
            value_slots: Vec::with_capacity(ctx.get_uniform_count(shader_index) as usize),
            texture_slots: Vec::new(),
            uniform_location_cache: Default::default(),
            temp_value: Default::default(),
        }
    }

    pub fn get_uniform_location(&mut self, name: &str) -> Option<UniformLocation> {
        if let Some(location) = self.uniform_location_cache.get(name) {
            *location
        } else {
            let location = self.ctx.get_uniform_location(self.shader_index, name);
            self.uniform_location_cache
                .insert(name.to_string(), location);
            location
        }
    }

    pub fn val<V, F>(&mut self, name: &str, f: F)
    where
        V: UniformValue,
        F: Fn(&T) -> V + 'static,
    {
        if let Some(location) = self.get_uniform_location(name) {
            self.value_slots.push((
                SlotData {
                    init: false,
                    previous: Default::default(),
                    location: location,
                },
                Box::new(
                    move |ctx: &BevyGlContext,
                          material: &T,
                          slot: &mut SlotData,
                          temp_value: &mut StackStack<u32, 16>| {
                        let v: V = f(material);
                        if !slot.init {
                            v.upload(ctx, &slot.location);
                            slot.init = true;
                        } else {
                            v.read_raw(temp_value);
                            if temp_value != &slot.previous {
                                std::mem::swap(&mut slot.previous, temp_value);
                                v.upload(ctx, &slot.location);
                            }
                        }
                    },
                ),
            ));
        }
    }

    pub fn tex<F>(&mut self, name: &str, f: F)
    where
        F: Fn(&T) -> &Option<Handle<Image>> + 'static,
    {
        if let Some(location) = self.get_uniform_location(name) {
            self.texture_slots.push((location, Box::new(f)))
        }
    }
    pub fn run(&mut self, material: &T) {
        for (slot, f) in &mut self.value_slots {
            f(&self.ctx, material, slot, &mut self.temp_value)
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

    pub fn reset_slot_cache(&mut self) {
        for (slot, _f) in &mut self.value_slots {
            slot.init = false;
        }
    }

    /// Uploads immediately if location is found
    pub fn upload<V>(&mut self, name: &str, v: V)
    where
        V: UniformValue,
    {
        if let Some(location) = self.get_uniform_location(name) {
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

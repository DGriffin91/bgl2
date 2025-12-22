pub mod mesh_util;
pub mod prepare_image;
pub mod prepare_mesh;
pub mod render;
pub mod unifrom_slot_builder;

use std::hash::Hash;
use std::hash::Hasher;
use std::rc::Rc;

use bevy::{platform::collections::HashMap, prelude::*};

use glow::ActiveAttribute;
use glow::ActiveUniform;
use glow::Buffer;
use glow::HasContext;

#[cfg(target_arch = "wasm32")]
use winit::platform::web::WindowExtWebSys;

pub type ShaderIndex = u32;

pub struct BevyGlContext {
    pub gl: Rc<glow::Context>,
    #[cfg(not(target_arch = "wasm32"))]
    pub gl_context: Option<glutin::context::PossiblyCurrentContext>,
    #[cfg(not(target_arch = "wasm32"))]
    pub gl_surface: Option<glutin::surface::Surface<glutin::surface::WindowSurface>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub gl_display: Option<glutin::display::Display>,
    pub shader_cache: Vec<glow::Program>,
    pub shader_cache_map: HashMap<u64, ShaderIndex>,
}

impl Drop for BevyGlContext {
    fn drop(&mut self) {
        unsafe {
            for program in &self.shader_cache {
                self.gl.delete_program(*program)
            }

            // TODO keep buffers in BevyGlContext and drop those too?

            #[cfg(not(target_arch = "wasm32"))]
            {
                drop(self.gl_surface.take());
                drop(self.gl_display.take());
                glutin::prelude::PossiblyCurrentGlContext::make_not_current(
                    self.gl_context.take().unwrap(),
                )
                .unwrap();
            }
        };
    }
}

impl BevyGlContext {
    pub fn new(
        #[allow(unused_variables)] bevy_window: &Window,
        winit_window: &bevy::window::WindowWrapper<winit::window::Window>,
    ) -> BevyGlContext {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let vsync = match bevy_window.present_mode {
                bevy::window::PresentMode::AutoVsync => true,
                bevy::window::PresentMode::AutoNoVsync => false,
                bevy::window::PresentMode::Fifo => true,
                bevy::window::PresentMode::FifoRelaxed => true,
                bevy::window::PresentMode::Immediate => false,
                bevy::window::PresentMode::Mailbox => false,
            };

            use glutin::{
                config::{ConfigSurfaceTypes, ConfigTemplateBuilder, GlConfig},
                context::{ContextApi, ContextAttributesBuilder},
                display::{Display, DisplayApiPreference},
                prelude::{GlDisplay, NotCurrentGlContext},
                surface::{GlSurface, SwapInterval},
            };
            use glutin_winit::GlWindow;
            use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
            use std::num::NonZeroU32;

            let raw_window = winit_window.window_handle().unwrap();
            let raw_display = winit_window.display_handle().unwrap();

            #[cfg(target_os = "windows")]
            let preference = DisplayApiPreference::Wgl(Some(raw_window.as_raw()));

            #[cfg(not(target_os = "windows"))]
            let preference = DisplayApiPreference::Egl;

            let gl_display = unsafe {
                Display::new(raw_display.as_raw(), preference).expect("Display::new failed")
            };

            // TODO https://github.com/rust-windowing/glutin/blob/master/glutin-winit/src/lib.rs
            let template = ConfigTemplateBuilder::default()
                // TODO depth buffer?
                .with_alpha_size(8)
                .with_surface_type(ConfigSurfaceTypes::WINDOW)
                .build();
            let gl_config = unsafe { gl_display.find_configs(template) }
                .unwrap()
                .reduce(|config, acc| {
                    if config.num_samples() > acc.num_samples() {
                        config
                    } else {
                        acc
                    }
                })
                .expect("No available configs");

            let context_attributes = ContextAttributesBuilder::new()
                .with_context_api(ContextApi::OpenGl(Some(glutin::context::Version {
                    major: 2,
                    minor: 1,
                })))
                .build(Some(raw_window.as_raw()));

            let not_current_gl_context = unsafe {
                gl_display
                    .create_context(&gl_config, &context_attributes)
                    .unwrap()
            };

            let attrs = winit_window
                .build_surface_attributes(Default::default())
                .unwrap();
            let gl_surface = unsafe {
                gl_display
                    .create_window_surface(&gl_config, &attrs)
                    .unwrap()
            };

            let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

            let gl = unsafe {
                glow::Context::from_loader_function_cstr(|s| gl_display.get_proc_address(s))
            };

            unsafe {
                let vendor = gl.get_parameter_string(glow::VENDOR);
                let renderer = gl.get_parameter_string(glow::RENDERER);
                let version = gl.get_parameter_string(glow::VERSION);

                println!("GL_VENDOR   : {}", vendor);
                println!("GL_RENDERER : {}", renderer);
                println!("GL_VERSION  : {}", version);
            }

            let interval = if vsync {
                SwapInterval::Wait(NonZeroU32::new(1).unwrap())
            } else {
                SwapInterval::DontWait
            };

            match gl_surface.set_swap_interval(&gl_context, interval) {
                Ok(_) => (),
                Err(e) => eprintln!("Couldn't set_swap_interval wait: {e}"),
            };

            let width = bevy_window.physical_size().x as u32;
            let height = bevy_window.physical_size().y as u32;

            unsafe { gl.viewport(0, 0, width as i32, height as i32) };

            BevyGlContext {
                gl: Rc::new(gl),
                gl_context: Some(gl_context),
                gl_surface: Some(gl_surface),
                gl_display: Some(gl_display),
                shader_cache: Default::default(),
                shader_cache_map: Default::default(),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            let canvas = winit_window.canvas().unwrap();

            let width = bevy_window.physical_size().x as u32;
            let height = bevy_window.physical_size().y as u32;

            canvas.set_width(width);
            canvas.set_height(height);

            let webgl_context = canvas
                .get_context("webgl")
                .unwrap()
                .unwrap()
                .dyn_into::<web_sys::WebGlRenderingContext>()
                .unwrap();
            let gl = glow::Context::from_webgl1_context(webgl_context);
            unsafe { gl.viewport(0, 0, width as i32, height as i32) };
            BevyGlContext {
                gl,
                shader_cache: Default::default(),
                shader_cache_map: Default::default(),
            }
        }
    }

    pub fn use_cached_program(&self, index: ShaderIndex) {
        unsafe { self.gl.use_program(Some(self.shader_cache[index as usize])) };
    }

    pub fn get_attrib_location(&self, shader_index: ShaderIndex, name: &str) -> Option<u32> {
        unsafe {
            self.gl
                .get_attrib_location(self.shader_cache[shader_index as usize], name)
        }
    }

    pub fn get_attribute_count(&self, shader_index: ShaderIndex) -> u32 {
        unsafe {
            self.gl
                .get_active_attributes(self.shader_cache[shader_index as usize])
        }
    }

    pub fn get_attribute(
        &self,
        shader_index: ShaderIndex,
        attribute_index: u32,
    ) -> Option<ActiveAttribute> {
        unsafe {
            self.gl
                .get_active_attribute(self.shader_cache[shader_index as usize], attribute_index)
        }
    }

    pub fn get_uniform_count(&self, shader_index: ShaderIndex) -> u32 {
        unsafe {
            self.gl
                .get_active_uniforms(self.shader_cache[shader_index as usize])
        }
    }

    pub fn get_uniform(
        &self,
        shader_index: ShaderIndex,
        uniform_index: u32,
    ) -> Option<ActiveUniform> {
        unsafe {
            self.gl
                .get_active_uniform(self.shader_cache[shader_index as usize], uniform_index)
        }
    }

    pub fn get_uniform_location(
        &self,
        shader_index: ShaderIndex,
        name: &str,
    ) -> Option<glow::UniformLocation> {
        unsafe {
            self.gl
                .get_uniform_location(self.shader_cache[shader_index as usize], name)
        }
    }

    pub fn shader_cached<F: Fn(&glow::Context, glow::Program)>(
        &mut self,
        vertex: &str,
        fragment: &str,
        before_link: F,
    ) -> ShaderIndex {
        let key = shader_key(vertex, fragment);
        if let Some(index) = self.shader_cache_map.get(&key) {
            *index
        } else {
            let shader = self.shader(vertex, fragment, before_link);
            let index = self.shader_cache.len() as u32;
            self.shader_cache.push(shader);
            index
        }
    }

    pub fn shader<F: Fn(&glow::Context, glow::Program)>(
        &self,
        vertex: &str,
        fragment: &str,
        before_link: F,
    ) -> glow::Program {
        unsafe {
            let program = self.gl.create_program().expect("Cannot create program");

            let shader_sources = [
                ("vertex", glow::VERTEX_SHADER, vertex),
                ("fragment", glow::FRAGMENT_SHADER, fragment),
            ];

            let mut shaders = Vec::with_capacity(shader_sources.len());

            for (stage_name, shader_type, shader_source) in shader_sources.iter() {
                let shader = self
                    .gl
                    .create_shader(*shader_type)
                    .expect("Cannot create shader");

                #[cfg(target_arch = "wasm32")]
                let preamble = "precision highp float;";
                #[cfg(not(target_arch = "wasm32"))]
                let preamble = "#version 120";

                self.gl
                    .shader_source(shader, &format!("{}\n{}", preamble, shader_source));

                self.gl.compile_shader(shader);

                if !self.gl.get_shader_compile_status(shader) {
                    panic!(
                        "{stage_name} shader compilation error: {}",
                        self.gl.get_shader_info_log(shader)
                    );
                }

                self.gl.attach_shader(program, shader);
                shaders.push(shader);
            }

            before_link(&self.gl, program);

            self.gl.link_program(program);

            if !self.gl.get_program_link_status(program) {
                panic!("{}", self.gl.get_program_info_log(program));
            }

            for shader in shaders {
                self.gl.detach_shader(program, shader);
                self.gl.delete_shader(shader);
            }

            program
        }
    }

    pub fn gen_vbo(&self, data: &[u8], usage: u32) -> Buffer {
        unsafe {
            let vbo = self.gl.create_buffer().unwrap();
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, data, usage);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            vbo
        }
    }

    pub fn gen_vbo_element(&self, data: &[u8], usage: u32) -> Buffer {
        unsafe {
            let vbo = self.gl.create_buffer().unwrap();
            self.gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(vbo));
            self.gl
                .buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, data, usage);
            self.gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
            vbo
        }
    }

    pub fn bind_vertex_attrib(
        &self,
        index: u32,
        element_count: u32,
        ty: AttribType,
        buffer: Buffer,
    ) {
        unsafe {
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
            self.gl.vertex_attrib_pointer_f32(
                index,
                element_count as i32,
                ty.gl_type(),
                false,
                element_count as i32 * ty.gl_type_bytes() as i32,
                0,
            );
            self.gl.enable_vertex_attrib_array(index);
        }
    }

    /// Only calls flush on webgl
    pub fn swap(&self) {
        unsafe { self.gl.flush() };
        #[cfg(not(target_arch = "wasm32"))]
        let _ = glutin::surface::GlSurface::swap_buffers(
            self.gl_surface.as_ref().unwrap(),
            self.gl_context.as_ref().unwrap(),
        );
    }
}

#[derive(Copy, Clone)]
pub enum AttribType {
    /// i8
    Byte,
    /// u8
    UnsignedByte,
    /// i16
    Short,
    /// u16
    UnsignedShort,
    /// f32
    Float,
}

impl AttribType {
    pub fn gl_type(&self) -> u32 {
        match &self {
            AttribType::Byte => glow::BYTE,
            AttribType::UnsignedByte => glow::UNSIGNED_BYTE,
            AttribType::Short => glow::SHORT,
            AttribType::UnsignedShort => glow::UNSIGNED_SHORT,
            AttribType::Float => glow::FLOAT,
        }
    }
    pub fn gl_type_bytes(&self) -> u32 {
        match &self {
            AttribType::Byte => 1,
            AttribType::UnsignedByte => 1,
            AttribType::Short => 2,
            AttribType::UnsignedShort => 2,
            AttribType::Float => 4,
        }
    }

    /// Unsupported types are replaced with the closest thing that is the same size in bytes.
    /// Ex: VertexFormat::Unorm8 => AttribType::UnsignedByte
    pub fn from_bevy_vertex_format(format: bevy::mesh::VertexFormat) -> Self {
        use bevy::mesh::VertexFormat;
        match format {
            VertexFormat::Uint8 => AttribType::UnsignedByte,
            VertexFormat::Uint8x2 => AttribType::UnsignedByte,
            VertexFormat::Uint8x4 => AttribType::UnsignedByte,
            VertexFormat::Sint8 => AttribType::Byte,
            VertexFormat::Sint8x2 => AttribType::Byte,
            VertexFormat::Sint8x4 => AttribType::Byte,
            VertexFormat::Unorm8 => AttribType::UnsignedByte,
            VertexFormat::Unorm8x2 => AttribType::UnsignedByte,
            VertexFormat::Unorm8x4 => AttribType::UnsignedByte,
            VertexFormat::Snorm8 => AttribType::Byte,
            VertexFormat::Snorm8x2 => AttribType::Byte,
            VertexFormat::Snorm8x4 => AttribType::Byte,
            VertexFormat::Uint16 => AttribType::UnsignedShort,
            VertexFormat::Uint16x2 => AttribType::UnsignedShort,
            VertexFormat::Uint16x4 => AttribType::UnsignedShort,
            VertexFormat::Sint16 => AttribType::Short,
            VertexFormat::Sint16x2 => AttribType::Short,
            VertexFormat::Sint16x4 => AttribType::Short,
            VertexFormat::Unorm16 => AttribType::UnsignedShort,
            VertexFormat::Unorm16x2 => AttribType::UnsignedShort,
            VertexFormat::Unorm16x4 => AttribType::UnsignedShort,
            VertexFormat::Snorm16 => AttribType::Short,
            VertexFormat::Snorm16x2 => AttribType::Short,
            VertexFormat::Snorm16x4 => AttribType::Short,
            VertexFormat::Float16 => AttribType::UnsignedShort,
            VertexFormat::Float16x2 => AttribType::UnsignedShort,
            VertexFormat::Float16x4 => AttribType::UnsignedShort,
            VertexFormat::Float32 => AttribType::Float,
            VertexFormat::Float32x2 => AttribType::Float,
            VertexFormat::Float32x3 => AttribType::Float,
            VertexFormat::Float32x4 => AttribType::Float,
            VertexFormat::Uint32 => AttribType::Float,
            VertexFormat::Uint32x2 => AttribType::Float,
            VertexFormat::Uint32x3 => AttribType::Float,
            VertexFormat::Uint32x4 => AttribType::Float,
            VertexFormat::Sint32 => AttribType::Float,
            VertexFormat::Sint32x2 => AttribType::Float,
            VertexFormat::Sint32x3 => AttribType::Float,
            VertexFormat::Sint32x4 => AttribType::Float,
            VertexFormat::Float64 => unimplemented!(),
            VertexFormat::Float64x2 => unimplemented!(),
            VertexFormat::Float64x3 => unimplemented!(),
            VertexFormat::Float64x4 => unimplemented!(),
            VertexFormat::Unorm10_10_10_2 => unimplemented!(),
            VertexFormat::Unorm8x4Bgra => unimplemented!(),
        }
    }
}

pub fn shader_key(vertex: &str, fragment: &str) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    vertex.hash(&mut hasher);
    fragment.hash(&mut hasher);
    hasher.finish()
}

pub trait UniformValue: Sized + 'static {
    fn upload(&self, ctx: &BevyGlContext, loc: &glow::UniformLocation);
}

impl UniformValue for bool {
    fn upload(&self, ctx: &BevyGlContext, loc: &glow::UniformLocation) {
        unsafe { ctx.gl.uniform_1_i32(Some(&loc), if *self { 1 } else { 0 }) };
    }
}

impl UniformValue for f32 {
    fn upload(&self, ctx: &BevyGlContext, loc: &glow::UniformLocation) {
        unsafe { ctx.gl.uniform_1_f32(Some(&loc), *self) };
    }
}

impl UniformValue for i32 {
    fn upload(&self, ctx: &BevyGlContext, loc: &glow::UniformLocation) {
        unsafe { ctx.gl.uniform_1_i32(Some(&loc), *self) };
    }
}

impl UniformValue for Vec2 {
    fn upload(&self, ctx: &BevyGlContext, loc: &glow::UniformLocation) {
        unsafe { ctx.gl.uniform_2_f32_slice(Some(&loc), &self.to_array()) };
    }
}

impl UniformValue for Vec3 {
    fn upload(&self, ctx: &BevyGlContext, loc: &glow::UniformLocation) {
        unsafe { ctx.gl.uniform_3_f32_slice(Some(&loc), &self.to_array()) };
    }
}

impl UniformValue for Vec4 {
    fn upload(&self, ctx: &BevyGlContext, loc: &glow::UniformLocation) {
        unsafe { ctx.gl.uniform_4_f32_slice(Some(&loc), &self.to_array()) };
    }
}

impl UniformValue for Mat4 {
    fn upload(&self, ctx: &BevyGlContext, loc: &glow::UniformLocation) {
        unsafe {
            ctx.gl
                .uniform_matrix_4_f32_slice(Some(&loc), false, &self.to_cols_array())
        };
    }
}

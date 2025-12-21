pub mod mesh_util;
pub mod prepare_image;
pub mod prepare_mesh;
pub mod render;

use std::hash::Hash;
use std::hash::Hasher;

use bevy::{platform::collections::HashMap, prelude::*};

use glow::Buffer;
use glow::HasContext;

#[cfg(target_arch = "wasm32")]
use winit::platform::web::WindowExtWebSys;

pub type ShaderIndex = u32;

pub struct BevyGlContext {
    pub gl: glow::Context,
    #[cfg(not(target_arch = "wasm32"))]
    pub gl_context: glutin::context::PossiblyCurrentContext,
    #[cfg(not(target_arch = "wasm32"))]
    pub gl_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
    pub shader_cache: Vec<glow::Program>,
    pub shader_cache_map: HashMap<u64, ShaderIndex>,
}

impl BevyGlContext {
    pub fn new(
        #[allow(unused_variables)] bevy_window: &Window,
        winit_window: &bevy::window::WindowWrapper<winit::window::Window>,
    ) -> BevyGlContext {
        #[cfg(not(target_arch = "wasm32"))]
        {
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

            match gl_surface
                .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
            {
                Ok(_) => (),
                Err(e) => eprintln!("Couldn't set_swap_interval wait: {e}"),
            };

            BevyGlContext {
                gl,
                gl_context,
                gl_surface,
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
                (glow::VERTEX_SHADER, vertex),
                (glow::FRAGMENT_SHADER, fragment),
            ];

            let mut shaders = Vec::with_capacity(shader_sources.len());

            for (shader_type, shader_source) in shader_sources.iter() {
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
                    panic!("{}", self.gl.get_shader_info_log(shader));
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

    /// Only calls flush on webgl
    pub fn swap(&self) {
        unsafe { self.gl.flush() };
        #[cfg(not(target_arch = "wasm32"))]
        glutin::surface::GlSurface::swap_buffers(&self.gl_surface, &self.gl_context).unwrap();
    }
}

pub fn shader_key(vertex: &str, fragment: &str) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    vertex.hash(&mut hasher);
    fragment.hash(&mut hasher);
    hasher.finish()
}

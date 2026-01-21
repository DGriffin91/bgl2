pub mod bevy_standard_lighting;
pub mod bevy_standard_material;
pub mod egui_plugin;
pub mod faststack;
pub mod macos_compat;
pub mod mesh_util;
pub mod phase_opaque;
pub mod phase_shadow;
pub mod phase_transparent;
pub mod plane_reflect;
pub mod prepare_image;
pub mod prepare_joints;
pub mod prepare_mesh;
pub mod render;
pub mod watchers;

extern crate self as bevy_opengl;

use anyhow::Error;
use anyhow::anyhow;
use bevy::platform::collections::HashSet;
use bytemuck::cast_slice;
use core::slice;
use glow::UniformLocation;
use std::any::TypeId;
use std::any::type_name;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::sync::Arc;
use wgpu_types::Face;

use bevy::{platform::collections::HashMap, prelude::*};

use glow::ActiveAttribute;
use glow::ActiveUniform;
use glow::Buffer;
use glow::HasContext;

#[cfg(target_arch = "wasm32")]
use winit::platform::web::WindowExtWebSys;

use crate::faststack::FastStack;
use crate::faststack::StackStack;
use crate::prepare_image::GpuImages;
use crate::watchers::Watchers;

pub type ShaderIndex = u32;

pub struct BevyGlContext {
    pub gl: Arc<glow::Context>,
    #[cfg(not(target_arch = "wasm32"))]
    pub gl_context: Option<glutin::context::PossiblyCurrentContext>,
    #[cfg(not(target_arch = "wasm32"))]
    pub gl_surface: Option<glutin::surface::Surface<glutin::surface::WindowSurface>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub gl_display: Option<glutin::display::Display>,
    pub shader_cache: Vec<glow::Program>,
    pub shader_cache_map: HashMap<u64, (ShaderIndex, Watchers)>,
    pub shader_snippets: HashMap<String, String>,
    pub has_glsl_cube_lod: bool, // TODO move
    pub has_cube_map_seamless: bool,
    pub last_cull_mode: Option<Face>,
    pub uniform_slot_map: HashMap<TypeId, Vec<Option<SlotData>>>,
    pub current_program: Option<glow::Program>,
    pub temp_slot_data: StackStack<u32, 16>,
    pub uniform_location_cache: HashMap<String, Option<UniformLocation>>,
    pub current_texture_slot_count: usize,
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
        #[cfg(feature = "gl21pipe")]
        unsafe {
            std::env::set_var(
                "__EGL_VENDOR_LIBRARY_FILENAMES",
                "/usr/share/glvnd/egl_vendor.d/50_mesa.json",
            );
            std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
            std::env::set_var("MESA_LOADER_DRIVER_OVERRIDE", "llvmpipe");
            std::env::set_var("MESA_GL_VERSION_OVERRIDE", "2.1");
            std::env::set_var("MESA_GLSL_VERSION_OVERRIDE", "120");
        }

        #[cfg(not(target_arch = "wasm32"))]
        let ctx = {
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
            #[cfg(target_os = "linux")]
            let preference = DisplayApiPreference::Egl;
            #[cfg(target_os = "macos")]
            let preference = DisplayApiPreference::Cgl;

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

            let has_cube_map_seamless = if gl
                .supported_extensions()
                .contains("GL_ARB_seamless_cube_map")
            {
                unsafe { gl.enable(glow::TEXTURE_CUBE_MAP_SEAMLESS) };
                true
            } else {
                false
            };

            let mut ctx = BevyGlContext {
                gl: Arc::new(gl),
                gl_context: Some(gl_context),
                gl_surface: Some(gl_surface),
                gl_display: Some(gl_display),
                shader_cache: Default::default(),
                shader_cache_map: Default::default(),
                shader_snippets: Default::default(),
                has_glsl_cube_lod: true,
                has_cube_map_seamless,
                last_cull_mode: None,
                uniform_slot_map: Default::default(),
                current_program: Default::default(),
                temp_slot_data: Default::default(),
                uniform_location_cache: Default::default(),
                current_texture_slot_count: 0,
            };
            ctx.test_for_glsl_lod();
            ctx
        };
        #[cfg(target_arch = "wasm32")]
        let ctx = {
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

            let has_glsl_cube_lod = webgl_context
                .get_extension("EXT_shader_texture_lod")
                .ok()
                .flatten()
                .is_some();

            let gl = glow::Context::from_webgl1_context(webgl_context);
            unsafe { gl.viewport(0, 0, width as i32, height as i32) };
            BevyGlContext {
                gl: Arc::new(gl),
                shader_cache: Default::default(),
                shader_cache_map: Default::default(),
                shader_snippets: Default::default(),
                has_glsl_cube_lod,
                has_cube_map_seamless: false,
            }
        };
        ctx
    }

    pub fn use_cached_program(&mut self, index: ShaderIndex) {
        self.uniform_slot_map.clear();
        self.temp_slot_data.clear();
        self.uniform_location_cache.clear();
        self.current_program = Some(self.shader_cache[index as usize]);
        self.current_texture_slot_count = 0;
        self.set_cull_mode(Some(Face::Back)); // Cull backfaces by default like bevy.
        unsafe { self.gl.use_program(self.current_program) };
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

    /// Get uniform location of any shader program in the cache
    pub fn get_shader_uniform_location(
        &self,
        shader_index: ShaderIndex,
        name: &str,
    ) -> Option<glow::UniformLocation> {
        unsafe {
            self.gl
                .get_uniform_location(self.shader_cache[shader_index as usize], name)
        }
    }

    /// Get uniform location for the currently bound shader program
    pub fn get_uniform_location(&mut self, name: &str) -> Option<glow::UniformLocation> {
        if let Some(location) = self.uniform_location_cache.get(name) {
            location.clone()
        } else {
            let location = unsafe {
                self.gl.get_uniform_location(
                    self.current_program
                        .expect("Need to run use_cached_program() before get_uniform_location()"),
                    name,
                )
            };
            self.uniform_location_cache
                .insert(name.to_string(), location.clone());
            location
        }
    }

    /// Uploads immediately if location is found
    pub fn load<V>(&mut self, name: &str, v: V)
    where
        V: UniformValue,
    {
        if let Some(location) = self.get_uniform_location(name) {
            v.load(&self.gl, &location);
        }
    }

    // Binding locations are optional. If they are not used get_uniform_location or UniformSlotBuilder must be used to
    // correlate binding names to numbers.
    pub fn shader_cached<P: AsRef<Path> + ?Sized>(
        &mut self,
        vertex: &P,
        fragment: &P,
        shader_defs: &[(&str, &str)],
    ) -> Option<ShaderIndex> {
        let key = shader_key(vertex.as_ref(), fragment.as_ref(), shader_defs);
        if let Some((index, watcher)) = self.shader_cache_map.get(&key) {
            if watcher.check() {
                let vertex_src = std::fs::read_to_string(vertex).unwrap();
                let fragment_src = std::fs::read_to_string(fragment).unwrap();
                let old_shader = self.shader_cache[*index as usize];
                let new_shader = self.shader(&vertex_src, &fragment_src, shader_defs);
                match new_shader {
                    Ok(shader) => {
                        self.shader_cache[*index as usize] = shader;
                        unsafe { self.gl.delete_program(old_shader) }
                    }
                    Err(e) => println!("{}", e),
                }
            }
            Some(*index)
        } else {
            let vertex_src = std::fs::read_to_string(vertex).unwrap();
            let fragment_src = std::fs::read_to_string(fragment).unwrap();
            let new_shader = self.shader(&vertex_src, &fragment_src, shader_defs);
            match new_shader {
                Ok(shader) => {
                    let index = self.shader_cache.len() as u32;
                    self.shader_cache.push(shader);
                    self.shader_cache_map.insert(
                        key,
                        (index, Watchers::new(&[vertex.as_ref(), fragment.as_ref()])),
                    );
                    Some(index)
                }
                Err(e) => {
                    println!("{}", e);
                    None
                }
            }
        }
    }

    #[must_use]
    pub fn shader(
        &self,
        vertex: &str,
        fragment: &str,
        shader_defs: &[(&str, &str)],
    ) -> Result<glow::Program, anyhow::Error> {
        unsafe {
            let program = self.gl.create_program().expect("Cannot create program");

            let mut vertex = vertex.to_string();
            let mut fragment = fragment.to_string();

            for (shader_type, shader_source) in [
                (glow::VERTEX_SHADER, &mut vertex),
                (glow::FRAGMENT_SHADER, &mut fragment),
            ] {
                #[cfg(target_arch = "wasm32")]
                let mut preamble = "precision highp float;\n".to_string();
                #[cfg(any(target_os = "windows", target_os = "linux"))]
                let mut preamble = "#version 120\n".to_string();
                #[cfg(target_os = "macos")]
                let mut preamble = "#version 330\n".to_string();

                shader_defs.into_iter().for_each(|shader_def| {
                    if !(shader_def.0.is_empty() && shader_def.1.is_empty()) {
                        preamble.push_str(&format!("#define {} {}\n", shader_def.0, shader_def.1));
                    }
                });

                #[cfg(target_arch = "wasm32")]
                preamble.push_str(&format!("#define WEBGL1\n"));

                if shader_type == glow::FRAGMENT_SHADER {
                    //let ext = self.gl.supported_extensions();
                    //#[cfg(not(target_arch = "wasm32"))]
                    //if ext.contains("GL_ARB_shader_texture_lod") {
                    if self.has_glsl_cube_lod {
                        #[cfg(target_arch = "wasm32")]
                        {
                            preamble.push_str("#extension GL_EXT_shader_texture_lod : enable\n");
                            preamble.push_str("vec4 textureCubeLod(samplerCube tex, vec3 dir, float lod) { return textureCubeLodEXT(tex, dir, lod); }\n");
                        }
                    } else {
                        #[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
                        {
                            preamble.push_str("vec4 textureCubeLod(samplerCube tex, vec3 dir, float lod) { return textureCube(tex, dir, lod); }\n");
                        }
                    }
                }

                let mut expanded_shader_source = String::with_capacity(shader_source.len() * 2);

                {
                    let mut already_included_snippets = HashSet::new();
                    for (i, line) in shader_source.lines().enumerate() {
                        if let Some(rest) = line.strip_prefix("#include") {
                            let snippet_name = rest.trim();
                            if let Some(snippet) = self.shader_snippets.get(snippet_name) {
                                if already_included_snippets.insert(snippet_name) {
                                    // TODO index snippets and use source-string-number
                                    expanded_shader_source.push_str(&format!("#line 0 1\n"));
                                    expanded_shader_source.push_str(snippet);
                                    expanded_shader_source.push_str("\n");
                                    expanded_shader_source.push_str(&format!("#line {i} 0\n"));
                                }
                            }
                        } else {
                            expanded_shader_source.push_str(line);
                            expanded_shader_source.push_str("\n");
                        }
                    }
                }

                *shader_source = format!("{}\n{}", preamble, expanded_shader_source);
            }

            #[cfg(target_os = "macos")]
            macos_compat::translate_shader_to_330(&mut vertex, &mut fragment);

            let shader_sources = [
                ("vertex", glow::VERTEX_SHADER, vertex),
                ("fragment", glow::FRAGMENT_SHADER, fragment),
            ];

            let mut shaders = Vec::with_capacity(shader_sources.len());

            for (stage_name, shader_type, shader_source) in shader_sources.iter() {
                let shader = self.gl.create_shader(*shader_type).map_err(Error::msg)?;

                self.gl.shader_source(shader, shader_source);

                self.gl.compile_shader(shader);

                if !self.gl.get_shader_compile_status(shader) {
                    return Err(anyhow!(
                        "{stage_name} shader compilation error: {}",
                        self.gl.get_shader_info_log(shader)
                    ));
                }

                self.gl.attach_shader(program, shader);
                shaders.push(shader);
            }

            self.gl.link_program(program);

            if !self.gl.get_program_link_status(program) {
                return Err(anyhow!("{}", self.gl.get_program_info_log(program)));
            }

            for shader in shaders {
                self.gl.detach_shader(program, shader);
                self.gl.delete_shader(shader);
            }

            Ok(program)
        }
    }

    pub fn add_snippet(&mut self, name: &str, src: &'static str) {
        self.shader_snippets
            .insert(String::from(name), String::from(src));
    }

    #[allow(dead_code)]
    fn test_for_glsl_lod(&mut self) {
        self.has_glsl_cube_lod = self
            .shader("void main() { gl_Position = vec4(0.0); }",
                "uniform samplerCube cube; void main() { gl_FragColor = textureCubeLod(cube, vec3(1.0), 0.0); }",
                Default::default()
            )
            .is_ok();
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

    pub fn clear_color_and_depth(&self, color: Option<Vec4>) {
        unsafe {
            self.gl.depth_mask(true);
            if let Some(color) = color {
                self.gl.clear_color(color.x, color.y, color.z, color.w);
            } else {
                self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            }
            self.gl.clear_depth_f32(0.0);
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        };
    }

    pub fn clear_color(&self, color: Option<Vec4>) {
        unsafe {
            self.gl.depth_mask(false);
            if let Some(color) = color {
                self.gl.clear_color(color.x, color.y, color.z, color.w);
            } else {
                self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            }
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        };
    }

    pub fn clear_depth(&self) {
        unsafe {
            self.gl.depth_mask(true);
            self.gl.clear_depth_f32(0.0);
            self.gl.clear(glow::DEPTH_BUFFER_BIT);
        };
    }

    pub fn start_alpha_blend(&self) {
        unsafe {
            self.gl.enable(glow::DEPTH_TEST);
            self.gl.enable(glow::BLEND);
            self.gl.depth_func(glow::GEQUAL);
            self.gl.depth_mask(false);
            self.gl.color_mask(true, true, true, true);
            self.gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
        }
    }

    /// It's not necessary to write depth after a prepass if everything is also included in opaque.
    pub fn start_opaque(&self, write_depth: bool) {
        unsafe {
            self.gl.enable(glow::DEPTH_TEST);
            self.gl.disable(glow::BLEND);
            self.gl.depth_func(glow::GEQUAL);
            self.gl.depth_mask(write_depth);
            self.gl.color_mask(true, true, true, true);
            self.gl.blend_func(glow::ZERO, glow::ONE);
        }
    }

    pub fn start_depth_only(&self) {
        unsafe {
            self.gl.enable(glow::DEPTH_TEST);
            self.gl.disable(glow::BLEND);
            self.gl.depth_func(glow::GEQUAL);
            self.gl.depth_mask(true);
            self.gl.color_mask(false, false, false, false);
            self.gl.blend_func(glow::ZERO, glow::ONE);
        }
    }

    pub fn set_cull_mode(&mut self, cull_mode: Option<Face>) {
        if self.last_cull_mode != cull_mode {
            self.last_cull_mode = cull_mode;
            unsafe {
                match cull_mode {
                    Some(face) => match face {
                        wgpu_types::Face::Front => {
                            self.gl.enable(glow::CULL_FACE);
                            self.gl.cull_face(glow::FRONT);
                        }
                        wgpu_types::Face::Back => {
                            self.gl.enable(glow::CULL_FACE);
                            self.gl.cull_face(glow::BACK);
                        }
                    },
                    None => {
                        self.gl.disable(glow::CULL_FACE);
                    }
                }
            }
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

pub fn flip_cull_mode(cull_mode: Option<Face>, flip: bool) -> Option<Face> {
    if flip && let Some(cull_mode) = cull_mode {
        Some(match cull_mode {
            Face::Front => Face::Back,
            Face::Back => Face::Front,
        })
    } else {
        None
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

pub fn shader_key(vertex: &Path, fragment: &Path, shader_defs: &[(&str, &str)]) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    vertex.hash(&mut hasher);
    fragment.hash(&mut hasher);
    shader_defs.hash(&mut hasher);
    hasher.finish()
}

pub trait UniformValue: Sized {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation);
    fn read_raw(&self, out: &mut StackStack<u32, 16>);
}

impl UniformValue for bool {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_1_i32(Some(&loc), if *self { 1 } else { 0 }) };
    }
    fn read_raw(&self, out: &mut StackStack<u32, 16>) {
        out.clear();
        out.push(if *self { 1 } else { 0 });
    }
}

impl UniformValue for f32 {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_1_f32(Some(&loc), *self) };
    }
    fn read_raw(&self, out: &mut StackStack<u32, 16>) {
        out.clear();
        out.push(self.to_bits());
    }
}

impl UniformValue for &[f32] {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_1_f32_slice(Some(&loc), &bytemuck::cast_slice(self)) };
    }
    fn read_raw(&self, _out: &mut StackStack<u32, 16>) {
        unimplemented!("read_raw for slices not supported");
    }
}

impl UniformValue for i32 {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_1_i32(Some(&loc), *self) };
    }
    fn read_raw(&self, out: &mut StackStack<u32, 16>) {
        out.clear();
        out.push(*self as u32);
    }
}

impl UniformValue for Vec2 {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_2_f32_slice(Some(&loc), &self.to_array()) };
    }
    fn read_raw(&self, out: &mut StackStack<u32, 16>) {
        out.clear();
        self.to_array().iter().for_each(|n| out.push(n.to_bits()));
    }
}

impl UniformValue for &[Vec2] {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_2_f32_slice(Some(&loc), &bytemuck::cast_slice(self)) };
    }
    fn read_raw(&self, _out: &mut StackStack<u32, 16>) {
        unimplemented!("read_raw for slices not supported");
    }
}

impl UniformValue for Vec3 {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_3_f32_slice(Some(&loc), &self.to_array()) };
    }
    fn read_raw(&self, out: &mut StackStack<u32, 16>) {
        out.clear();
        self.to_array().iter().for_each(|n| out.push(n.to_bits()));
    }
}

impl UniformValue for &[Vec3] {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_3_f32_slice(Some(&loc), &bytemuck::cast_slice(self)) };
    }
    fn read_raw(&self, _out: &mut StackStack<u32, 16>) {
        unimplemented!("read_raw for slices not supported");
    }
}

impl UniformValue for Vec4 {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_4_f32_slice(Some(&loc), &self.to_array()) };
    }
    fn read_raw(&self, out: &mut StackStack<u32, 16>) {
        out.clear();
        self.to_array().iter().for_each(|n| out.push(n.to_bits()));
    }
}

impl UniformValue for &[Vec4] {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_4_f32_slice(Some(&loc), &bytemuck::cast_slice(self)) };
    }
    fn read_raw(&self, _out: &mut StackStack<u32, 16>) {
        unimplemented!("read_raw for slices not supported");
    }
}

impl UniformValue for Mat4 {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe {
            gl.uniform_matrix_4_f32_slice(
                Some(&loc),
                false,
                cast_slice::<Mat4, f32>(slice::from_ref(self)),
            )
        };
    }
    fn read_raw(&self, out: &mut StackStack<u32, 16>) {
        out.clear();
        self.to_cols_array()
            .iter()
            .for_each(|n| out.push(n.to_bits()));
    }
}

impl UniformValue for &[Mat4] {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_matrix_4_f32_slice(Some(&loc), false, &bytemuck::cast_slice(self)) };
    }
    fn read_raw(&self, _out: &mut StackStack<u32, 16>) {
        unimplemented!("read_raw for slices not supported");
    }
}

impl UniformValue for LinearRgba {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_4_f32_slice(Some(&loc), &self.to_vec4().to_array()) };
    }
    fn read_raw(&self, out: &mut StackStack<u32, 16>) {
        out.clear();
        self.to_vec4()
            .to_array()
            .iter()
            .for_each(|n| out.push(n.to_bits()));
    }
}

impl UniformValue for Color {
    fn load(&self, gl: &glow::Context, loc: &glow::UniformLocation) {
        unsafe { gl.uniform_4_f32_slice(Some(&loc), &self.to_linear().to_vec4().to_array()) };
    }
    fn read_raw(&self, out: &mut StackStack<u32, 16>) {
        out.clear();
        self.to_linear()
            .to_vec4()
            .to_array()
            .iter()
            .for_each(|n| out.push(n.to_bits()));
    }
}

#[macro_export]
/// if target_arch = wasm32 or the bundle_shaders feature is enabled the shader strings will be included in the binary.
/// otherwise they will be hot reloaded when modified.
macro_rules! shader_cached {
    ($bevy_gl_context:expr, $vertex:expr, $fragment:expr, $shader_defs:expr) => {{
        #[cfg(not(any(target_arch = "wasm32", feature = "bundle_shaders")))]
        {
            let path = std::path::Path::new(file!()).parent().unwrap();
            $bevy_gl_context.shader_cached(&path.join($vertex), &path.join($fragment), $shader_defs)
        }

        #[cfg(any(target_arch = "wasm32", feature = "bundle_shaders"))]
        {
            let key = $crate::shader_key($vertex.as_ref(), $fragment.as_ref(), $shader_defs);
            if let Some((index, _)) = $bevy_gl_context.shader_cache_map.get(&key) {
                Some(*index)
            } else {
                if let Ok(shader) = $bevy_gl_context.shader(
                    &include_str!($vertex),
                    &include_str!($fragment),
                    $shader_defs,
                ) {
                    let index = $bevy_gl_context.shader_cache.len() as u32;
                    $bevy_gl_context.shader_cache.push(shader);
                    $bevy_gl_context
                        .shader_cache_map
                        .insert(key, (index, Default::default()));
                    Some(index)
                } else {
                    None
                }
            }
        }
    }};
}

pub trait UniformSet {
    // TODO depth only variant? When the depth only variant uses an ifdef it should be that the locations return None
    // TODO could cache a names/location set for each combination of ShaderProgram+TypeId.
    /// if bool is true uniform is texture
    fn names() -> &'static [(&'static str, bool)];
    /// The index for load should correspond to the order returned from names()
    /// location is where this value should be put
    /// if the current item differs from prev_value bind it and update prev_value
    /// temp_value is scratch space to load your value to so it can be compared to prev_value
    fn load(
        &self,
        gl: &glow::Context,
        gpu_images: &GpuImages,
        index: u32,
        slot: &mut SlotData,
        temp_value: &mut StackStack<u32, 16>,
    );
}

#[inline]
pub fn load_if_new<T: UniformValue>(
    v: &T,
    gl: &glow::Context,
    slot: &mut SlotData,
    temp: &mut StackStack<u32, 16>,
) {
    match slot {
        SlotData::Uniform {
            init,
            previous,
            location,
        } => {
            v.read_raw(temp);
            if !*init || temp != previous {
                *init = true;
                std::mem::swap(previous, temp);
                v.load(&gl, &location);
            }
        }
        _ => panic!("Expected uniform"),
    }
}

#[inline]
pub fn load_tex_if_new(tex: &Tex, gl: &glow::Context, gpu_images: &GpuImages, slot: &mut SlotData) {
    match slot {
        SlotData::Texture {
            target,
            texture_slot,
            previous,
            location,
        } => {
            let mut texture = gpu_images.placeholder.unwrap();
            match tex {
                Tex::Bevy(image_h) => {
                    if let Some(image_h) = image_h {
                        if let Some(t) = gpu_images.mapping.get(&image_h.id()) {
                            texture = t.0;
                            *target = t.1;
                        }
                    }
                }
                Tex::Gl(t) => {
                    texture = *t;
                }
            }
            unsafe {
                if let Some(previous) = previous.as_ref() {
                    if previous == &texture {
                        return;
                    }
                }
                // TODO needs to use info from the texture to actually setup correctly
                gl.active_texture(glow::TEXTURE0 + *texture_slot);
                gl.bind_texture(*target, Some(texture));
                gl.uniform_1_i32(Some(&location), *texture_slot as i32);
                *previous = Some(texture);
            }
        }
        _ => panic!("Expected uniform"),
    }
}

impl BevyGlContext {
    pub fn map_uniform_set_locations<T: UniformSet + 'static>(&mut self) {
        let current_program = self
            .current_program
            .expect("Need to run use_cached_program() before map_uniform_set_locations()");

        let locations = T::names()
            .iter()
            .map(|(name, is_texture)| unsafe {
                self.gl
                    .get_uniform_location(current_program, name)
                    .map(|location| {
                        if *is_texture {
                            let slot = SlotData::Texture {
                                target: glow::TEXTURE_2D,
                                texture_slot: self.current_texture_slot_count as u32,
                                previous: None,
                                location,
                            };
                            self.current_texture_slot_count += 1;
                            slot
                        } else {
                            SlotData::Uniform {
                                init: false,
                                previous: Default::default(),
                                location,
                            }
                        }
                    })
            })
            .collect::<Vec<_>>();

        self.uniform_slot_map.insert(TypeId::of::<T>(), locations);
    }
    pub fn bind_uniforms_set<T: UniformSet + 'static>(&mut self, gpu_images: &GpuImages, v: &T) {
        for (index, slot) in self
            .uniform_slot_map
            .get_mut(&TypeId::of::<T>())
            .expect(&format!(
                "Uniform map missing. Call ctx.map_uniform_set_locations::<{}>() before bind_uniforms_set().",
                type_name::<T>()
            ))
            .iter_mut()
            .enumerate()
        {
            if let Some(slot) = slot {
                v.load(
                    &self.gl,
                    gpu_images,
                    index as u32,
                    slot,
                    &mut self.temp_slot_data,
                );
            }
        }
    }
    #[inline]
    /// Loads the texture in the next available slot. Returns the texture slot and location if the location is found.
    /// Call set_tex() to update the texture at this slot.
    pub fn load_tex(
        &mut self,
        name: &str,
        tex: &Tex,
        gpu_images: &GpuImages,
    ) -> Option<(u32, glow::UniformLocation)> {
        let mut texture = gpu_images.placeholder.unwrap();
        let mut target = glow::TEXTURE_2D;

        let Some(location) = self.get_uniform_location(name) else {
            return None;
        };
        let texture_slot = self.current_texture_slot_count as u32;
        self.current_texture_slot_count += 1;

        match tex {
            Tex::Bevy(image_h) => {
                if let Some(image_h) = image_h {
                    if let Some(t) = gpu_images.mapping.get(&image_h.id()) {
                        texture = t.0;
                        target = t.1;
                    }
                }
            }
            Tex::Gl(t) => {
                texture = *t;
            }
        }
        unsafe {
            // TODO needs to use info from the texture to actually setup correctly
            self.gl.active_texture(glow::TEXTURE0 + texture_slot);
            self.gl.bind_texture(target, Some(texture));
            self.gl.uniform_1_i32(Some(&location), texture_slot as i32);
        }
        Some((texture_slot, location))
    }

    #[inline]
    pub fn set_tex(
        &self,
        tex: &Tex,
        gpu_images: &GpuImages,
        slot_location: (u32, glow::UniformLocation),
    ) {
        let mut texture = gpu_images.placeholder.unwrap();
        let mut target = glow::TEXTURE_2D;
        match tex {
            Tex::Bevy(image_h) => {
                if let Some(image_h) = image_h {
                    if let Some(t) = gpu_images.mapping.get(&image_h.id()) {
                        texture = t.0;
                        target = t.1;
                    }
                }
            }
            Tex::Gl(t) => {
                texture = *t;
            }
        }
        unsafe {
            // TODO needs to use info from the texture to actually setup correctly
            self.gl.active_texture(glow::TEXTURE0 + slot_location.0);
            self.gl.bind_texture(target, Some(texture));
            self.gl
                .uniform_1_i32(Some(&slot_location.1), slot_location.0 as i32);
        }
    }
}

pub enum SlotData {
    Uniform {
        init: bool,
        previous: StackStack<u32, 16>,
        location: glow::UniformLocation,
    },
    Texture {
        target: u32,
        texture_slot: u32,
        previous: Option<glow::Texture>,
        location: glow::UniformLocation,
    },
}

#[derive(Clone)]
pub enum Tex {
    // TODO use references for Handle<Image> so clone isn't needed
    Bevy(Option<Handle<Image>>),
    Gl(glow::Texture),
}

#[cfg(not(target_arch = "wasm32"))]
impl From<glow::NativeTexture> for Tex {
    fn from(tex: glow::NativeTexture) -> Self {
        Tex::Gl(tex)
    }
}

#[cfg(target_arch = "wasm32")]
impl From<glow::WebTextureKey> for Tex {
    fn from(tex: glow::WebTextureKey) -> Self {
        Tex::Gl(tex)
    }
}

impl From<Option<Handle<Image>>> for Tex {
    fn from(handle: Option<Handle<Image>>) -> Self {
        Tex::Bevy(handle)
    }
}

impl From<Handle<Image>> for Tex {
    fn from(handle: Handle<Image>) -> Self {
        Tex::Bevy(Some(handle))
    }
}

#[macro_export]
macro_rules! load_match {
    ($index:expr, $gl:expr, $gpu:expr, $slot:expr, $temp:expr, {
        $( $i:literal => $kind:ident($expr:expr) ),* $(,)?
    }) => {{
        match $index {
            $(
                $i => load_match!(@do $kind, $expr, $gl, $gpu, $slot, $temp),
            )*
            _ => unreachable!(),
        }
    }};

    (@do value, $expr:expr, $gl:expr, $_gpu:expr, $slot:expr, $temp:expr) => {
        load_if_new(&($expr), $gl, $slot, $temp)
    };

    (@do tex, $expr:expr, $gl:expr, $gpu:expr, $slot:expr, $_temp:expr) => {
        load_tex_if_new(&($expr), $gl, $gpu, $slot)
    };
}

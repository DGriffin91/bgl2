use bevy::{
    ecs::system::SystemState,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    winit::WINIT_WINDOWS,
};
use bytemuck::cast_slice;
use glow::HasContext;

#[cfg(not(target_arch = "wasm32"))]
type GlProgram = glow::NativeProgram;

#[cfg(target_arch = "wasm32")]
type GlProgram = glow::WebProgramKey;
#[cfg(target_arch = "wasm32")]
use winit::platform::web::WindowExtWebSys;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins.set(RenderPlugin {
            render_creation: WgpuSettings {
                backends: None,
                ..default()
            }
            .into(),
            ..default()
        }),))
        .add_systems(Startup, (init, triangle).chain())
        .add_systems(Update, update)
        .run();
}

fn init(world: &mut World, params: &mut SystemState<Query<(Entity, &mut Window)>>) {
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let mut windows = params.get_mut(world);
        #[allow(unused_variables)]
        let (bevy_window_entity, bevy_window) = windows.single_mut().unwrap();
        let Some(winit_window) = winit_windows.get_window(bevy_window_entity) else {
            panic!("No Window Found")
        };

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

            gl_surface
                .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
                .unwrap();
            world.insert_non_send_resource(GlContext {
                gl,
                gl_context,
                gl_surface,
            });
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
            world.insert_non_send_resource(GlContext { gl });
        }
    });
}

struct GlContext {
    gl: glow::Context,
    #[cfg(not(target_arch = "wasm32"))]
    gl_context: glutin::context::PossiblyCurrentContext,
    #[cfg(not(target_arch = "wasm32"))]
    gl_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
}

fn triangle(world: &mut World) {
    let mut ctx = world.non_send_resource_mut::<GlContext>();
    let gl = &mut ctx.gl;

    let vertex_array = unsafe {
        gl.create_vertex_array()
            .expect("Cannot create vertex array")
    };
    unsafe { gl.bind_vertex_array(Some(vertex_array)) };

    let program = unsafe { gl.create_program().expect("Cannot create program") };

    let vertex_shader_source = r#"
attribute vec2 a_position;
varying vec2 vert;

void main() {
    vert = a_position;
    gl_Position = vec4(a_position - vec2(0.5, 0.5), 0.0, 1.0);
}
        "#;

    let fragment_shader_source = r#"

varying vec2 vert;

void main() {
    gl_FragColor = vec4(vert, 0.0, 1.0);
}
    "#;

    let shader_sources = [
        (glow::VERTEX_SHADER, vertex_shader_source),
        (glow::FRAGMENT_SHADER, fragment_shader_source),
    ];

    let mut shaders = Vec::with_capacity(shader_sources.len());

    for (shader_type, shader_source) in shader_sources.iter() {
        let shader = unsafe {
            gl.create_shader(*shader_type)
                .expect("Cannot create shader")
        };

        #[cfg(target_arch = "wasm32")]
        let preamble = "precision highp float;";
        #[cfg(not(target_arch = "wasm32"))]
        let preamble = "#version 120";

        unsafe { gl.shader_source(shader, &format!("{}\n{}", preamble, shader_source)) };
        unsafe { gl.compile_shader(shader) };
        unsafe {
            if !gl.get_shader_compile_status(shader) {
                panic!("{}", gl.get_shader_info_log(shader));
            }
        }
        unsafe { gl.attach_shader(program, shader) };
        shaders.push(shader);
    }

    unsafe { gl.link_program(program) };
    unsafe {
        if !gl.get_program_link_status(program) {
            panic!("{}", gl.get_program_info_log(program));
        }
    }

    for shader in shaders {
        unsafe { gl.detach_shader(program, shader) };
        unsafe { gl.delete_shader(shader) };
    }

    world.insert_non_send_resource(TriangleProgram { program });
}

struct TriangleProgram {
    program: GlProgram,
}

fn update(pgm: NonSend<TriangleProgram>, ctx: NonSend<GlContext>) {
    unsafe {
        ctx.gl.use_program(Some(pgm.program));
        ctx.gl.clear_color(0.0, 0.0, 0.0, 1.0);

        let vbo = ctx.gl.create_buffer().unwrap();
        let triangle_vertices = [0.5f32, 1.0, 0.0, 0.0, 1.0, 0.0];
        ctx.gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        ctx.gl.clear(glow::COLOR_BUFFER_BIT);
        ctx.gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            cast_slice(&triangle_vertices),
            glow::STATIC_DRAW,
        );

        let vao = ctx.gl.create_vertex_array().unwrap();
        let pos_loc = ctx
            .gl
            .get_attrib_location(pgm.program, "a_position")
            .unwrap();
        ctx.gl.bind_vertex_array(Some(vao));
        ctx.gl.enable_vertex_attrib_array(pos_loc);
        ctx.gl
            .vertex_attrib_pointer_f32(pos_loc, 2, glow::FLOAT, false, 8, 0);

        ctx.gl.draw_arrays(glow::TRIANGLES, 0, 3);
    };

    #[cfg(not(target_arch = "wasm32"))]
    glutin::surface::GlSurface::swap_buffers(&ctx.gl_surface, &ctx.gl_context).unwrap();
}

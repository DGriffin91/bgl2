use bevy::{
    ecs::system::SystemState,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    winit::WINIT_WINDOWS,
};
use bevy_opengl::{BevyGlContext, GlProgram};
use bytemuck::cast_slice;
use glow::HasContext;

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
    if world.contains_non_send::<BevyGlContext>() {
        return;
    }
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let mut windows = params.get_mut(world);

        let (bevy_window_entity, bevy_window) = windows.single_mut().unwrap();
        let Some(winit_window) = winit_windows.get_window(bevy_window_entity) else {
            warn!("No Window Found");
            return;
        };

        let ctx = BevyGlContext::new(&bevy_window, winit_window);
        world.insert_non_send_resource(ctx);
    });
}

fn triangle(world: &mut World) {
    let mut ctx = world.non_send_resource_mut::<BevyGlContext>();
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

fn update(pgm: NonSend<TriangleProgram>, ctx: NonSend<BevyGlContext>) {
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

    ctx.swap();
}

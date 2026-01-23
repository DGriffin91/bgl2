use bevy::{ecs::system::SystemState, prelude::*, winit::WINIT_WINDOWS};
use bevy_opengl::command_encoder::{CommandEncoder, CommandEncoderPlugin, CommandEncoderSender};
use bevy_opengl::{BevyGlContext, WindowInitData, shader_cached};
use bytemuck::cast_slice;
use glow::HasContext;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

#[cfg(not(target_arch = "wasm32"))]
use glutin_winit::GlWindow;

#[cfg(target_arch = "wasm32")]
use winit::platform::web::WindowExtWebSys;

fn main() {
    console_error_panic_hook::set_once();
    App::new()
        .add_plugins((
            MinimalPlugins,
            bevy::input::InputPlugin,
            AssetPlugin::default(),
            bevy::a11y::AccessibilityPlugin,
            bevy::winit::WinitPlugin::default(),
            bevy::scene::ScenePlugin,
            WindowPlugin::default(),
            ImagePlugin::default_linear(),
            CommandEncoderPlugin,
        ))
        .add_systems(Startup, init)
        .add_systems(PostUpdate, update)
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

        let window_init_data = WindowInitData {
            #[cfg(not(target_arch = "wasm32"))]
            attrs: winit_window
                .build_surface_attributes(Default::default())
                .unwrap()
                .clone(),
            #[cfg(target_arch = "wasm32")]
            canvas: winit_window.canvas().unwrap(),
            raw_window: winit_window.window_handle().unwrap().clone().as_raw(),
            raw_display: winit_window.display_handle().unwrap().clone().as_raw(),
            present_mode: bevy_window.present_mode,
            width: bevy_window.physical_size().x as u32,
            height: bevy_window.physical_size().y as u32,
        };

        let sender = CommandEncoderSender::new(window_init_data);

        #[cfg(not(target_arch = "wasm32"))]
        world.insert_resource(sender);
        #[cfg(target_arch = "wasm32")]
        world.insert_non_send_resource(sender);
    });
}

fn update(mut cmd: ResMut<CommandEncoder>) {
    cmd.record(|ctx, _world| {
        let shader_index = shader_cached!(
            ctx,
            "../assets/shaders/tri.vert",
            "../assets/shaders/tri.frag",
            &[],
            &[]
        )
        .unwrap();
        unsafe {
            ctx.use_cached_program(shader_index);
            ctx.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            ctx.gl.clear(glow::COLOR_BUFFER_BIT);

            let vbo = ctx.gl.create_buffer().unwrap();
            let triangle_vertices = [0.5f32, 1.0, 0.0, 0.0, 1.0, 0.0];
            ctx.gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            ctx.gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_slice(&triangle_vertices),
                glow::STATIC_DRAW,
            );

            let pos_loc = ctx.get_attrib_location(shader_index, "a_position").unwrap();

            ctx.gl.enable_vertex_attrib_array(pos_loc);
            ctx.gl
                .vertex_attrib_pointer_f32(pos_loc, 2, glow::FLOAT, false, 8, 0);

            ctx.gl.draw_arrays(glow::TRIANGLES, 0, 3);

            ctx.gl.delete_buffer(vbo);
        };

        ctx.swap();
    });
}

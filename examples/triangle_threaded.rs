use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread;

use bevy::{ecs::system::SystemState, prelude::*, winit::WINIT_WINDOWS};
use bevy_opengl::{BevyGlContext, WindowInitData, shader_cached};
use bytemuck::cast_slice;
use glow::HasContext;
use glutin_winit::GlWindow;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

fn main() {
    App::new()
        .init_resource::<CommandEncoder>()
        .add_plugins((
            MinimalPlugins,
            bevy::input::InputPlugin,
            AssetPlugin::default(),
            bevy::a11y::AccessibilityPlugin,
            bevy::winit::WinitPlugin::default(),
            bevy::scene::ScenePlugin,
            WindowPlugin::default(),
            ImagePlugin::default_linear(),
            //bevy::mesh::MeshPlugin,
            //bevy::camera::CameraPlugin,
            //bevy::gltf::GltfPlugin::default(),
        ))
        .add_systems(Startup, init)
        .add_systems(PostUpdate, update)
        .add_systems(Last, send)
        .run();
}

fn init(world: &mut World, params: &mut SystemState<Query<(Entity, &mut Window)>>) {
    if world.contains_non_send::<BevyGlContext>() {
        return;
    }

    let (sender, receiver) = sync_channel::<CommandEncoder>(1);
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let mut windows = params.get_mut(world);

        let (bevy_window_entity, bevy_window) = windows.single_mut().unwrap();
        let Some(winit_window) = winit_windows.get_window(bevy_window_entity) else {
            warn!("No Window Found");
            return;
        };

        let window_init_data = WindowInitData {
            attrs: winit_window
                .build_surface_attributes(Default::default())
                .unwrap()
                .clone(),
            raw_window: winit_window.window_handle().unwrap().clone().as_raw(),
            raw_display: winit_window.display_handle().unwrap().clone().as_raw(),
            present_mode: bevy_window.present_mode,
            width: bevy_window.physical_size().x as u32,
            height: bevy_window.physical_size().y as u32,
        };

        thread::spawn(move || {
            let mut ctx = BevyGlContext::new(window_init_data);
            loop {
                if let Ok(mut msg) = receiver.recv() {
                    for cmd in msg.commands.iter_mut() {
                        cmd(&mut ctx)
                    }
                }
            }
        });
    });

    world.insert_resource(CommandEncoderSender { sender });
}

#[derive(Resource)]
pub struct CommandEncoderSender {
    sender: SyncSender<CommandEncoder>,
}

#[derive(Resource, Default)]
pub struct CommandEncoder {
    pub commands: Vec<Box<dyn FnMut(&mut BevyGlContext) + Send + Sync>>,
}

fn update(mut cmd: ResMut<CommandEncoder>) {
    cmd.commands.push(Box::new(|ctx: &mut BevyGlContext| {
        let shader_index = shader_cached!(
            ctx,
            "../assets/shaders/tri.vert",
            "../assets/shaders/tri.frag",
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
    }));
}

fn send(mut cmd: ResMut<CommandEncoder>, sender: Res<CommandEncoderSender>) {
    let mut new_cmd_encoder = CommandEncoder::default();
    std::mem::swap(&mut *cmd, &mut new_cmd_encoder);
    sender.sender.send(new_cmd_encoder).unwrap();
}

use bevy::{
    ecs::system::SystemState,
    prelude::*,
    winit::{WINIT_WINDOWS, WakeUp},
};
use bevy_opengl::BevyGlContext;
use bytemuck::cast_slice;
use glow::HasContext;

fn main() {
    App::new()
        .add_plugins((
            MinimalPlugins,
            bevy::input::InputPlugin,
            AssetPlugin::default(),
            bevy::a11y::AccessibilityPlugin,
            bevy::winit::WinitPlugin::<WakeUp>::default(),
            bevy::scene::ScenePlugin,
            WindowPlugin::default(),
            ImagePlugin::default_linear(),
            //bevy::mesh::MeshPlugin,
            //bevy::camera::CameraPlugin,
            //bevy::gltf::GltfPlugin::default(),
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

        let ctx = BevyGlContext::new(&bevy_window, winit_window);
        world.insert_non_send_resource(ctx);
    });
}

fn update(mut ctx: If<NonSendMut<BevyGlContext>>) {
    let shader_index = ctx
        .shader_cached("examples/tri.vert", "examples/tri.frag")
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
}

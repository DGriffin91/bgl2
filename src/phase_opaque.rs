use bevy::prelude::*;

use crate::{
    BevyGlContext,
    plane_reflect::{ReflectionPlane, copy_reflection_texture},
    render::{RenderPhase, RenderRunner, RenderSet},
};

pub struct OpaquePhasePlugin;

impl Plugin for OpaquePhasePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (
                render_reflect_opaque.in_set(RenderSet::RenderReflectOpaque),
                (copy_reflection_texture, render_opaque)
                    .chain()
                    .in_set(RenderSet::RenderOpaque),
            ),
        );
    }
}

fn render_reflect_opaque(world: &mut World) {
    let mut planes = world.query::<&ReflectionPlane>();
    if planes.iter(world).len() == 0 {
        return;
    }
    *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::ReflectOpaque;
    // Don't need to clear color or depth opaque always clears both at start.
    opaque(world);
}

fn render_opaque(world: &mut World) {
    *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::Opaque;
    opaque(world);
}

// During the opaque pass the registered systems also write any transparent items to the DeferredAlphaBlendDraws.
fn opaque(world: &mut World) {
    let clear_color = world.resource::<ClearColor>().clone();
    let ctx = world.get_non_send_resource_mut::<BevyGlContext>().unwrap();
    ctx.start_opaque(true);
    ctx.clear_color_and_depth(Some(clear_color.to_srgba().to_vec4()));

    let Some(runner) = world.remove_resource::<RenderRunner>() else {
        return;
    };

    for system in &runner.prepare_registry {
        let _ = world.run_system(*system);
    }

    // Systems fill in phase data while they draw opaque
    for (_type_id, system) in &runner.render_registry {
        let _ = world.run_system(*system);
    }

    world.insert_resource(runner);
}

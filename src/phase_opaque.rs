use bevy::prelude::*;

use crate::{
    BevyGlContext,
    render::{RenderPhase, RenderRunner, RenderSet},
};

pub struct OpaquePhasePlugin;

impl Plugin for OpaquePhasePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, render_opaque.in_set(RenderSet::RenderOpaque));
    }
}

// During the opaque pass the registered systems also write any transparent items to the DeferredAlphaBlendDraws.
fn render_opaque(world: &mut World) {
    let ctx = world.get_non_send_resource_mut::<BevyGlContext>().unwrap();
    ctx.start_opaque(true);
    ctx.clear_color_and_depth();

    *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::Opaque;

    let Some(runner) = world.remove_resource::<RenderRunner>() else {
        return;
    };

    // Systems fill in phase data while they draw opaque
    for (_type_id, system) in &runner.registry {
        let _ = world.run_system(*system);
    }

    world.insert_resource(runner);
}

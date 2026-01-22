use bevy::{core_pipeline::prepass::DepthPrepass, prelude::*};

use crate::{
    command_encoder::CommandEncoder,
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
    clear_color_and_depth(world);
    let mut query = world.query::<(&Camera3d, &DepthPrepass)>();
    let depth_prepass_enabled = query.iter(world).len() > 0;
    if depth_prepass_enabled {
        *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::ReflectDepthPrepass;
        opaque(world, true, true)
    }
    *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::ReflectOpaque;
    opaque(world, false, !depth_prepass_enabled);
}

fn render_opaque(world: &mut World) {
    clear_color_and_depth(world);
    let mut query = world.query::<(&Camera3d, &DepthPrepass)>();
    let depth_prepass_enabled = query.iter(world).len() > 0;
    if depth_prepass_enabled {
        *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::DepthPrepass;
        opaque(world, true, true)
    }
    *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::Opaque;
    opaque(world, false, !depth_prepass_enabled);
}

// During the opaque pass the registered systems also write any transparent items to the DeferredAlphaBlendDraws.
fn opaque(world: &mut World, depth_prepass: bool, write_depth: bool) {
    let mut cmd = world.resource_mut::<CommandEncoder>();

    if depth_prepass {
        cmd.start_depth_only();
    } else {
        cmd.start_opaque(write_depth);
    }

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

fn clear_color_and_depth(world: &mut World) {
    // Seems faster to clear these together
    let color = world.resource::<ClearColor>().clone();
    let mut cmd = world.resource_mut::<CommandEncoder>();
    cmd.clear_color_and_depth(Some(color.to_srgba().to_vec4()));
}

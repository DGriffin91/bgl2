use bevy::{
    app::{App, Plugin, PostUpdate},
    ecs::{system::Single, world::World},
    prelude::{If, NonSendMut, Query},
    window::Window,
};
use bevy_egui::{EguiContext, EguiPlugin, EguiPostUpdateSet, EguiRenderOutput};

use bevy::prelude::IntoScheduleConfigs;
use egui_glow::{Painter, ShaderVersion};

use crate::{BevyGlContext, render::RenderSet};

#[derive(Default)]
pub struct GlowEguiPlugin;

impl Plugin for GlowEguiPlugin {
    fn build(&self, app: &mut App) {
        // TODO any reason to let the user add EguiPlugin?
        app.add_plugins(EguiPlugin::default())
            .add_systems(PostUpdate, setup.in_set(RenderSet::Pipeline))
            .add_systems(
                PostUpdate,
                egui_render
                    .in_set(RenderSet::RenderUi)
                    .after(EguiPostUpdateSet::ProcessOutput),
            );
    }
}

struct EguiGlow {
    painter: Painter,
}

fn setup(world: &mut World) {
    if world.get_non_send_resource::<EguiGlow>().is_some() {
        return;
    }

    let Some(ctx) = world.get_non_send_resource::<BevyGlContext>() else {
        return;
    };

    #[cfg(target_arch = "wasm32")]
    let shader_version = ShaderVersion::Es100;
    #[cfg(not(target_arch = "wasm32"))]
    let shader_version = ShaderVersion::Gl120;

    world.insert_non_send_resource(EguiGlow {
        painter: Painter::new(ctx.gl.clone(), "", Some(shader_version), false).unwrap(),
    });
}

fn egui_render(
    window: Single<&Window>,
    mut egui_glow: If<NonSendMut<EguiGlow>>,
    mut contexts: Query<(&mut EguiContext, &mut EguiRenderOutput)>,
) {
    let width = window.physical_width().max(1);
    let height = window.physical_height().max(1);

    for (mut context, render_output) in contexts.iter_mut() {
        egui_glow.painter.paint_and_update_textures(
            [width, height],
            context.get_mut().pixels_per_point(),
            &render_output.paint_jobs,
            &render_output.textures_delta,
        );
    }
}

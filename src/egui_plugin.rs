use bevy::{
    app::{App, Plugin, PostUpdate},
    ecs::{
        system::{ResMut, Single},
        world::World,
    },
    prelude::*,
    window::Window,
};
use bevy_egui::{EguiContext, EguiPlugin, EguiPostUpdateSet, EguiRenderOutput};

use bevy::prelude::IntoScheduleConfigs;
use egui_glow::{Painter, ShaderVersion};

use crate::{command_encoder::CommandEncoder, render::RenderSet};

#[derive(Default)]
pub struct GlowEguiPlugin;

impl Plugin for GlowEguiPlugin {
    fn build(&self, app: &mut App) {
        // TODO any reason to let the user add EguiPlugin?
        app.add_plugins(EguiPlugin::default())
            .add_systems(Startup, setup.in_set(RenderSet::Init))
            .add_systems(
                PostUpdate,
                egui_render
                    .in_set(RenderSet::RenderUi)
                    .after(EguiPostUpdateSet::ProcessOutput),
            );
    }
}

#[derive(Resource)]
pub struct EguiPainter(pub Painter);

fn setup(world: &mut World) {
    let mut cmd = world.resource_mut::<CommandEncoder>();
    cmd.record(move |ctx, world| {
        #[cfg(target_arch = "wasm32")]
        let shader_version = ShaderVersion::Es100;
        #[cfg(not(target_arch = "wasm32"))]
        let shader_version = ShaderVersion::Gl120;
        world.insert_resource(EguiPainter(
            Painter::new(ctx.gl.clone(), "", Some(shader_version), false).unwrap(),
        ));
    });
}

fn egui_render(
    window: Single<&Window>,
    mut contexts: Query<(&mut EguiContext, &mut EguiRenderOutput)>,
    mut cmd: ResMut<CommandEncoder>,
) {
    let width = window.physical_width().max(1);
    let height = window.physical_height().max(1);

    for (mut context, render_output) in contexts.iter_mut() {
        let paint_jobs = render_output.paint_jobs.clone();
        let textures_delta = render_output.textures_delta.clone();
        let pixels_per_point = context.get_mut().pixels_per_point();
        cmd.record(move |_ctx, world| {
            let painter = &mut world.resource_mut::<EguiPainter>().0;
            painter.paint_and_update_textures(
                [width, height],
                pixels_per_point,
                &paint_jobs,
                &textures_delta,
            );
        });
    }
}

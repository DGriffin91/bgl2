use std::num::NonZeroU32;

use bevy::{prelude::*, window::WindowResized};
use glow::HasContext;

#[cfg(not(target_arch = "wasm32"))]
use glutin::surface::GlSurface;

use crate::{BevyGlContext, prepare_image::PrepareImagePlugin, prepare_mesh::PrepareMeshPlugin};

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum RenderSet {
    Init,
    Pipeline,
    Acquire,
    Prepare,
    PrepareView,
    Render,
    Present,
}

pub struct OpenGLRenderPlugin;

impl Plugin for OpenGLRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((PrepareMeshPlugin, PrepareImagePlugin));

        // TODO reference: https://github.com/bevyengine/bevy/pull/22144
        app.configure_sets(Startup, (RenderSet::Init, RenderSet::Pipeline));
        app.configure_sets(
            PostUpdate,
            (
                RenderSet::Init,
                RenderSet::Pipeline,
                RenderSet::Acquire,
                RenderSet::Prepare,
                RenderSet::PrepareView,
                RenderSet::Render,
                RenderSet::Present,
            )
                .chain()
                .after(TransformSystems::Propagate),
        );

        app.add_systems(PostUpdate, present.in_set(RenderSet::Present));
    }
}

fn present(
    ctx: NonSend<BevyGlContext>,
    resized: MessageReader<WindowResized>,
    bevy_window: Single<&Window>,
) {
    ctx.swap();
    if resized.len() > 0 {
        let width = bevy_window.physical_width().max(1);
        let height = bevy_window.physical_height().max(1);
        //let present_mode = bevy_window.present_mode; // TODO update

        #[cfg(not(target_arch = "wasm32"))]
        {
            // TODO support wasm?
            unsafe { ctx.gl.viewport(0, 0, width as i32, height as i32) };
            unsafe { ctx.gl.scissor(0, 0, width as i32, height as i32) };
            ctx.gl_surface.as_ref().unwrap().resize(
                ctx.gl_context.as_ref().unwrap(),
                NonZeroU32::new(width).unwrap(),
                NonZeroU32::new(height).unwrap(),
            );
        }
    }
}

use std::any::TypeId;

use bevy::{
    ecs::system::SystemId, platform::collections::HashMap, prelude::*, window::WindowResized,
};

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
    RenderOpaque,
    RenderTransparent,
    Present,
}

pub struct OpenGLRenderPlugin;

impl Plugin for OpenGLRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderRunner>()
            .init_resource::<RenderPhase>()
            .init_resource::<DeferredAlphaBlendDraws>()
            .add_plugins((PrepareMeshPlugin, PrepareImagePlugin));

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
                RenderSet::RenderOpaque,
                RenderSet::RenderTransparent,
                RenderSet::Present,
            )
                .chain()
                .after(TransformSystems::Propagate),
        );

        app.add_systems(PostUpdate, clear.in_set(RenderSet::PrepareView));
        app.add_systems(PostUpdate, render_opaque.in_set(RenderSet::RenderOpaque));
        app.add_systems(
            PostUpdate,
            render_transparent.in_set(RenderSet::RenderTransparent),
        );
        app.add_systems(PostUpdate, present.in_set(RenderSet::Present));
    }
}

fn clear(ctx: NonSend<BevyGlContext>) {
    ctx.clear_color_and_depth();
}

fn present(
    ctx: NonSend<BevyGlContext>,
    resized: MessageReader<WindowResized>,
    #[allow(unused)] bevy_window: Single<&Window>,
) {
    ctx.swap();
    if resized.len() > 0 {
        // TODO support wasm?
        #[cfg(not(target_arch = "wasm32"))]
        {
            use glow::HasContext;
            use std::num::NonZeroU32;
            let width = bevy_window.physical_width().max(1);
            let height = bevy_window.physical_height().max(1);
            //let present_mode = bevy_window.present_mode; // TODO update
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

#[derive(Resource, Default)]
pub enum RenderPhase {
    #[default]
    Opaque,
    Transparent(Entity),
}

#[derive(Resource, Default, Deref, DerefMut)]
pub struct DeferredAlphaBlendDraws(pub Vec<(f32, Entity, TypeId)>);

#[derive(Default, Resource)]
pub struct RenderRunner {
    pub registry: HashMap<TypeId, SystemId>,
}

impl RenderRunner {
    pub fn register<T: 'static>(&mut self, system: SystemId) {
        self.registry.insert(TypeId::of::<T>(), system);
    }
}

// During the opaque pass the registered systems also write any transparent items to the DeferredAlphaBlendDraws.
fn render_opaque(world: &mut World) {
    world
        .get_non_send_resource_mut::<BevyGlContext>()
        .unwrap()
        .start_opaque(true);
    *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::Opaque;

    let Some(runner) = world.remove_resource::<RenderRunner>() else {
        return;
    };

    world
        .get_resource_mut::<DeferredAlphaBlendDraws>()
        .unwrap()
        .0
        .clear();

    // Systems fill in phase data while they draw opaque
    for (_type_id, system) in &runner.registry {
        let _ = world.run_system(*system);
    }

    world.insert_resource(runner);
}

fn render_transparent(world: &mut World) {
    world
        .get_non_send_resource_mut::<BevyGlContext>()
        .unwrap()
        .start_alpha_blend();

    let Some(runner) = world.remove_resource::<RenderRunner>() else {
        return;
    };

    let mut draws = world.remove_resource::<DeferredAlphaBlendDraws>().unwrap();

    draws.0.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    // Systems read from RenderPhase to draw transparent
    for (_dist, entity, type_id) in &draws.0 {
        *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::Transparent(*entity);
        let _ = world.run_system(*runner.registry.get(type_id).unwrap());
    }

    draws.0.clear();
    world.insert_resource(draws);
    world.insert_resource(runner);
}

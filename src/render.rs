use std::any::TypeId;

use bevy::{
    ecs::system::{SystemId, SystemState},
    platform::collections::HashMap,
    prelude::*,
    window::WindowResized,
    winit::WINIT_WINDOWS,
};

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
        app.configure_sets(Startup, (RenderSet::Init, RenderSet::Pipeline).chain());
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

        app.add_systems(Startup, init_gl.in_set(RenderSet::Init));
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

#[derive(Resource, Default, PartialEq, Eq)]
pub enum RenderPhase {
    #[default]
    Opaque,
    Transparent,
}

#[derive(Resource, Default)]
pub struct DeferredAlphaBlendDraws {
    pub deferred: Vec<(f32, Entity, TypeId)>,
    pub next: Vec<Entity>,
}

impl DeferredAlphaBlendDraws {
    // Defer an entity to be drawn in the alpha blend phase
    pub fn defer<T: ?Sized + 'static>(&mut self, distance: f32, entity: Entity) {
        self.deferred.push((distance, entity, TypeId::of::<T>()));
    }

    // Take the current set of alpha blend entities to be drawn
    pub fn take(&mut self) -> Vec<Entity> {
        std::mem::take(&mut self.next)
    }
}

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
        .deferred
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
    *world.get_resource_mut::<RenderPhase>().unwrap() = RenderPhase::Transparent;

    let Some(runner) = world.remove_resource::<RenderRunner>() else {
        return;
    };

    {
        let mut draws = world.get_resource_mut::<DeferredAlphaBlendDraws>().unwrap();
        draws
            .deferred
            .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        draws.next.clear();
    }

    let mut current_type_id = None;
    let mut last = false;
    // Draw deferred transparent
    loop {
        let mut draws = world.get_resource_mut::<DeferredAlphaBlendDraws>().unwrap();
        // collect draws off the end of draws.deferred on to draws.next until we hit a different id, then submit those
        // before collecting the next set
        loop {
            if let Some((dist, entity, type_id)) = draws.deferred.pop() {
                if let Some(last_type_id) = current_type_id {
                    if last_type_id == type_id {
                        draws.next.push(entity);
                    } else {
                        draws.deferred.push((dist, entity, type_id));
                        current_type_id = None;
                        break;
                    }
                } else {
                    draws.next.clear();
                    draws.next.push(entity);
                    current_type_id = Some(type_id);
                }
            } else {
                last = true;
                break;
            }
        }

        if let Some(current_type_id) = current_type_id {
            let _ = world.run_system(*runner.registry.get(&current_type_id).unwrap());
        } else {
            break;
        }
        if last {
            break;
        }
    }

    let ctx = world.non_send_resource::<BevyGlContext>();
    unsafe { ctx.gl.bind_vertex_array(None) };
    world.insert_resource(runner);
}

pub fn init_gl(world: &mut World, params: &mut SystemState<Query<(Entity, &mut Window)>>) {
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

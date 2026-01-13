use std::any::TypeId;

use bevy::{
    ecs::system::{SystemId, SystemState},
    image::{CompressedImageFormatSupport, CompressedImageFormats},
    light::SimulationLightSystems,
    platform::collections::HashMap,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    window::WindowResized,
    winit::WINIT_WINDOWS,
};
use glow::HasContext;

use bevy_egui::egui::ahash::HashSet;
#[cfg(not(target_arch = "wasm32"))]
use glutin::surface::GlSurface;

use crate::{
    BevyGlContext, phase_opaque::OpaquePhasePlugin, phase_shadow::ShadowPhasePlugin,
    phase_transparent::TransparentPhasePlugin, plane_reflect::PlaneReflectPlugin,
    prepare_image::PrepareImagePlugin, prepare_mesh::PrepareMeshPlugin,
};

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum RenderSet {
    Init,
    Pipeline,
    Acquire,
    Prepare,
    RenderShadow,
    RenderReflectOpaque,
    RenderReflectTransparent,
    RenderOpaque,
    RenderTransparent,
    RenderUi,
    Present,
}

pub struct OpenGLRenderPlugins;

impl Plugin for OpenGLRenderPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            OpenGLMinimalRenderPlugin,
            ShadowPhasePlugin,
            OpaquePhasePlugin,
            TransparentPhasePlugin,
            PlaneReflectPlugin,
        ));
    }
}

pub struct OpenGLMinimalRenderPlugin;

impl Plugin for OpenGLMinimalRenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CompressedImageFormatSupport(CompressedImageFormats::BC)) // TODO query?
            .init_resource::<RenderRunner>()
            .init_resource::<RenderPhase>()
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
                RenderSet::RenderShadow,
                RenderSet::RenderReflectOpaque,
                RenderSet::RenderReflectTransparent,
                RenderSet::RenderOpaque,
                RenderSet::RenderTransparent,
                RenderSet::RenderUi,
                RenderSet::Present,
            )
                .chain()
                .after(TransformSystems::Propagate)
                .after(SimulationLightSystems::UpdateDirectionalLightCascades),
        );

        app.add_systems(Startup, init_gl.in_set(RenderSet::Init));
        app.add_systems(PostUpdate, present.in_set(RenderSet::Present));
    }
}

fn present(
    ctx: NonSend<BevyGlContext>,
    resized: MessageReader<WindowResized>,
    mut bevy_window: Single<(Entity, &mut Window)>,
) {
    ctx.swap();
    #[allow(unused)]
    let (bevy_window_entity, bevy_window) = &mut *bevy_window;
    let width = bevy_window.physical_width().max(1);
    let height = bevy_window.physical_height().max(1);
    if resized.len() > 0 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::num::NonZeroU32;
            //let present_mode = bevy_window.present_mode; // TODO update
            unsafe { ctx.gl.viewport(0, 0, width as i32, height as i32) };
            unsafe { ctx.gl.scissor(0, 0, width as i32, height as i32) };
            ctx.gl_surface.as_ref().unwrap().resize(
                ctx.gl_context.as_ref().unwrap(),
                NonZeroU32::new(width).unwrap(),
                NonZeroU32::new(height).unwrap(),
            );
        }
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;
            WINIT_WINDOWS.with_borrow(|winit_windows| {
                let Some(winit_window) = winit_windows.get_window(*bevy_window_entity) else {
                    warn!("No Window Found");
                    return;
                };
                winit_window.canvas().unwrap().set_width(width);
                winit_window.canvas().unwrap().set_height(height);
                unsafe { ctx.gl.viewport(0, 0, width as i32, height as i32) };
                unsafe { ctx.gl.scissor(0, 0, width as i32, height as i32) };
            });
        }
    }
}

#[derive(Resource, Default, PartialEq, Eq, Clone, Copy)]
pub enum RenderPhase {
    Shadow,
    ReflectOpaque,
    ReflectTransparent,
    #[default]
    Opaque,
    Transparent,
}

impl RenderPhase {
    pub fn can_use_camera_frustum_cull(&self) -> bool {
        match self {
            RenderPhase::Shadow | RenderPhase::ReflectOpaque | RenderPhase::ReflectTransparent => {
                false
            }
            RenderPhase::Opaque | RenderPhase::Transparent => true,
        }
    }
    pub fn reflection(&self) -> bool {
        match self {
            RenderPhase::ReflectOpaque | RenderPhase::ReflectTransparent => true,
            RenderPhase::Shadow | RenderPhase::Opaque | RenderPhase::Transparent => false,
        }
    }
    pub fn opaque(&self) -> bool {
        match self {
            RenderPhase::ReflectOpaque | RenderPhase::Opaque => true,
            _ => false,
        }
    }
    pub fn transparent(&self) -> bool {
        match self {
            RenderPhase::ReflectTransparent | RenderPhase::Transparent => true,
            _ => false,
        }
    }
    pub fn read_reflect(&self) -> bool {
        match self {
            RenderPhase::Shadow | RenderPhase::ReflectOpaque | RenderPhase::ReflectTransparent => {
                false
            }
            RenderPhase::Opaque | RenderPhase::Transparent => true,
        }
    }
}

#[derive(Default, Resource)]
pub struct RenderRunner {
    pub render_registry: HashMap<TypeId, SystemId>,
    pub prepare_registry: HashSet<SystemId>,
}

impl RenderRunner {
    pub fn register_render<T: 'static>(&mut self, system: SystemId) {
        self.render_registry.insert(TypeId::of::<T>(), system);
    }
    pub fn register_prepare(&mut self, system: SystemId) {
        self.prepare_registry.insert(system);
    }
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

pub fn register_render_system<T: 'static, M>(
    world: &mut World,
    system: impl IntoSystem<(), (), M> + 'static,
) {
    let system_id = world.register_system(system);
    world
        .get_resource_mut::<RenderRunner>()
        .unwrap()
        .register_render::<T>(system_id);
}

/// Systems registered here are run at the start of each phase
pub fn register_prepare_system<M>(world: &mut World, system: impl IntoSystem<(), (), M> + 'static) {
    let system_id = world.register_system(system);
    world
        .get_resource_mut::<RenderRunner>()
        .unwrap()
        .register_prepare(system_id);
}

pub fn default_plugins_no_render_backend() -> bevy::app::PluginGroupBuilder {
    DefaultPlugins.set(RenderPlugin {
        render_creation: WgpuSettings {
            backends: None,
            ..default()
        }
        .into(),
        ..default()
    })
}

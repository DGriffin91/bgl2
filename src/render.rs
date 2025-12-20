use bevy::{mesh::MeshPlugin, prelude::*};

use crate::prepare_mesh::PrepareMeshPlugin;

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
        app.add_plugins(PrepareMeshPlugin);

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
    }
}

use bevy::{
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use bevy_opengl::{egui_plugin::GlowEguiPlugin, render::OpenGLRenderPlugin};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(RenderPlugin {
                render_creation: WgpuSettings {
                    backends: None,
                    ..default()
                }
                .into(),
                ..default()
            }),
            OpenGLRenderPlugin,
        ))
        .add_plugins(GlowEguiPlugin::default())
        // Systems that create Egui widgets should be run during the `CoreSet::Update` set,
        // or after the `EguiSet::BeginFrame` system (which belongs to the `CoreSet::PreUpdate` set).
        .add_systems(EguiPrimaryContextPass, ui_example_system)
        .add_systems(Startup, setup)
        .run();
}

fn ui_example_system(mut contexts: EguiContexts) {
    egui::Window::new("Hello").show(contexts.ctx_mut().unwrap(), |ui| {
        ui.label("world");
    });
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera3d::default());
}

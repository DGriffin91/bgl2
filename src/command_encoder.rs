#[cfg(not(target_arch = "wasm32"))]
use std::{
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    thread,
};

use bevy::prelude::*;
use glow::HasContext;
use wgpu_types::Face;

use crate::{
    BevyGlContext, WindowInitData,
    prepare_image::{GpuImages, TextureRef},
    render::RenderSet,
};

pub struct CommandEncoderPlugin;

impl Plugin for CommandEncoderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandEncoder>()
            .add_systems(PostUpdate, send.in_set(RenderSet::SubmitEncoder));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn send(mut cmd: ResMut<CommandEncoder>, sender: Res<CommandEncoderSender>) {
    let mut new_cmd_encoder = CommandEncoder::default();
    std::mem::swap(&mut *cmd, &mut new_cmd_encoder);
    sender.sender.send(new_cmd_encoder).unwrap();
}

#[cfg(target_arch = "wasm32")]
fn send(mut cmd: ResMut<CommandEncoder>, mut sender: NonSendMut<CommandEncoderSender>) {
    // Could just clear cmd.commands but want to match the multi-threaded version
    cmd.commands.iter_mut().for_each(|cmd| cmd(&mut sender.ctx));
    *cmd = CommandEncoder::default();
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource)]
pub struct CommandEncoderSender {
    pub sender: SyncSender<CommandEncoder>,
}

#[cfg(target_arch = "wasm32")]
pub struct CommandEncoderSender {
    pub ctx: BevyGlContext,
}

impl CommandEncoderSender {
    pub fn new(window_init_data: WindowInitData) -> CommandEncoderSender {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (sender, receiver) = sync_channel::<CommandEncoder>(1);
            CommandEncoderSender::receiver_thread(window_init_data, receiver);
            CommandEncoderSender { sender }
        }
        #[cfg(target_arch = "wasm32")]
        {
            CommandEncoderSender {
                ctx: BevyGlContext::new(window_init_data),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn receiver_thread(window_init_data: WindowInitData, receiver: Receiver<CommandEncoder>) {
        thread::spawn(move || {
            let mut ctx = BevyGlContext::new(window_init_data);
            let mut world = World::new();
            loop {
                if let Ok(mut msg) = receiver.recv() {
                    msg.commands
                        .iter_mut()
                        .for_each(|cmd| cmd(&mut ctx, &mut world));
                }
            }
        });
    }
}

#[derive(Resource, Default)]
pub struct CommandEncoder {
    pub commands: Vec<Box<dyn FnMut(&mut BevyGlContext, &mut World) + Send + Sync>>,
    pub next_buffer_id: usize,
}

impl CommandEncoder {
    pub fn record<F>(&mut self, f: F)
    where
        F: FnMut(&mut BevyGlContext, &mut World) + Send + Sync + 'static,
    {
        self.commands.push(Box::new(f));
    }

    pub fn bevy_image(&mut self, image: Image) -> TextureRef {
        let texture_ref = TextureRef::new();
        let return_tex = texture_ref.clone();
        self.record(move |ctx, world| {
            world.resource_mut::<GpuImages>().add_bevy_image_set_ref(
                ctx,
                None,
                &image,
                &texture_ref,
            );
        });
        return_tex
    }

    pub fn clear_color_and_depth(&mut self, color: Option<Vec4>) {
        self.record(move |ctx, _world| {
            ctx.clear_color_and_depth(color);
        });
    }

    pub fn clear_color(&mut self, color: Option<Vec4>) {
        self.record(move |ctx, _world| {
            ctx.clear_color(color);
        });
    }

    pub fn clear_depth(&mut self) {
        self.record(move |ctx, _world| {
            ctx.clear_depth();
        });
    }

    pub fn start_alpha_blend(&mut self) {
        self.record(move |ctx, _world| {
            ctx.start_alpha_blend();
        });
    }

    /// It's not necessary to write depth after a prepass if everything is also included in opaque.
    pub fn start_opaque(&mut self, write_depth: bool) {
        self.record(move |ctx, _world| {
            ctx.start_opaque(write_depth);
        });
    }

    pub fn start_depth_only(&mut self) {
        self.record(move |ctx, _world| {
            ctx.start_depth_only();
        });
    }

    pub fn set_cull_mode(&mut self, cull_mode: Option<Face>) {
        self.record(move |ctx, _world| {
            ctx.set_cull_mode(cull_mode);
        });
    }

    /// Only calls flush on webgl
    pub fn swap(&mut self) {
        self.record(move |ctx, _world| {
            ctx.swap();
        });
    }

    pub fn delete_texture_ref(&mut self, texture_ref: TextureRef) {
        self.record(move |ctx, world| unsafe {
            if let Some((tex, _target)) = world
                .resource_mut::<GpuImages>()
                .texture_from_ref(&texture_ref)
            {
                ctx.gl.delete_texture(tex);
            }
        });
    }

    pub fn delete_image(&mut self, id: AssetId<Image>) {
        self.record(move |ctx, world| {
            if let Some(tex) = world.resource_mut::<GpuImages>().bevy_textures.remove(&id) {
                unsafe { ctx.gl.delete_texture(tex.0) };
            }
        });
    }
}

use std::{
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    thread,
};

use bevy::prelude::*;
use glow::HasContext;
use wgpu_types::Face;

use crate::{BevyGlContext, WindowInitData, prepare_image::TextureRef, render::RenderSet};

pub struct CommandEncoderPlugin;

impl Plugin for CommandEncoderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandEncoder>()
            .add_systems(PostUpdate, send.in_set(RenderSet::SubmitEncoder));
    }
}

fn send(mut cmd: ResMut<CommandEncoder>, sender: Res<CommandEncoderSender>) {
    let mut new_cmd_encoder = CommandEncoder::default();
    std::mem::swap(&mut *cmd, &mut new_cmd_encoder);
    sender.sender.send(new_cmd_encoder).unwrap();
}

#[derive(Resource)]
pub struct CommandEncoderSender {
    pub sender: SyncSender<CommandEncoder>,
}

impl CommandEncoderSender {
    pub fn new(window_init_data: WindowInitData) -> CommandEncoderSender {
        let (sender, receiver) = sync_channel::<CommandEncoder>(1);
        CommandEncoderSender::receiver_thread(window_init_data, receiver);
        CommandEncoderSender { sender }
    }

    fn receiver_thread(window_init_data: WindowInitData, receiver: Receiver<CommandEncoder>) {
        thread::spawn(move || {
            let mut ctx = BevyGlContext::new(window_init_data);
            loop {
                if let Ok(mut msg) = receiver.recv() {
                    msg.commands.iter_mut().for_each(|cmd| cmd(&mut ctx));
                }
            }
        });
    }
}

#[derive(Resource, Default)]
pub struct CommandEncoder {
    pub commands: Vec<Box<dyn FnMut(&mut BevyGlContext) + Send + Sync>>,
    pub next_buffer_id: usize,
}

impl CommandEncoder {
    pub fn record<F>(&mut self, f: F)
    where
        F: FnMut(&mut BevyGlContext) + Send + Sync + 'static,
    {
        self.commands.push(Box::new(f));
    }

    pub fn bevy_image(&mut self, image: Image) -> TextureRef {
        let texture_ref = TextureRef::new();
        let return_tex = texture_ref.clone();
        self.record(move |ctx| {
            ctx.add_bevy_image_set_ref(None, &image, &texture_ref);
        });
        return_tex
    }

    pub fn clear_color_and_depth(&mut self, color: Option<Vec4>) {
        self.record(move |ctx| {
            ctx.clear_color_and_depth(color);
        });
    }

    pub fn clear_color(&mut self, color: Option<Vec4>) {
        self.record(move |ctx| {
            ctx.clear_color(color);
        });
    }

    pub fn clear_depth(&mut self) {
        self.record(move |ctx| {
            ctx.clear_depth();
        });
    }

    pub fn start_alpha_blend(&mut self) {
        self.record(move |ctx| {
            ctx.start_alpha_blend();
        });
    }

    /// It's not necessary to write depth after a prepass if everything is also included in opaque.
    pub fn start_opaque(&mut self, write_depth: bool) {
        self.record(move |ctx| {
            ctx.start_opaque(write_depth);
        });
    }

    pub fn start_depth_only(&mut self) {
        self.record(move |ctx| {
            ctx.start_depth_only();
        });
    }

    pub fn set_cull_mode(&mut self, cull_mode: Option<Face>) {
        self.record(move |ctx| {
            ctx.set_cull_mode(cull_mode);
        });
    }

    /// Only calls flush on webgl
    pub fn swap(&mut self) {
        self.record(move |ctx| {
            ctx.swap();
        });
    }

    pub fn delete_texture_ref(&mut self, texture_ref: TextureRef) {
        self.record(move |ctx| unsafe {
            if let Some((tex, _target)) = ctx.texture_from_ref(&texture_ref) {
                ctx.gl.delete_texture(tex);
            }
        });
    }

    pub fn delete_image(&mut self, id: AssetId<Image>) {
        self.record(move |ctx| {
            if let Some(tex) = ctx.image.bevy_textures.remove(&id) {
                unsafe { ctx.gl.delete_texture(tex.0) };
            }
        });
    }
}

use std::{
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    thread,
};

use bevy::prelude::*;

use crate::{BevyGlContext, WindowInitData, render::RenderSet};

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
}

use bevy::{
    image::{CompressedImageFormatSupport, CompressedImageFormats},
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

use glow::{HasContext, PixelUnpackData};

use crate::{BevyGlContext, render::RenderSet};

/// Handles uploading bevy Image assets to the GPU
pub struct PrepareImagePlugin;

impl Plugin for PrepareImagePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CompressedImageFormatSupport(CompressedImageFormats::BC))
            .init_resource::<GpuImages>()
            .add_systems(
                PostUpdate,
                send_images_to_gpu.chain().in_set(RenderSet::Prepare),
            );
    }
}

#[derive(Resource, Default)]
pub struct GpuImages {
    pub images: Vec<glow::Texture>,
    pub mapping: HashMap<AssetId<Image>, u32>,
    pub updated_this_frame: bool,
}

pub fn send_images_to_gpu(
    mut gpu_images: ResMut<GpuImages>,
    images: Res<Assets<Image>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    ctx: If<NonSend<BevyGlContext>>,
) {
    gpu_images.updated_this_frame = false;

    let mut updated: HashSet<AssetId<Image>> = HashSet::new();
    for event in image_events.read() {
        match event {
            AssetEvent::Modified { id } | AssetEvent::Added { id } => {
                updated.insert(id.clone());
            }
            AssetEvent::Removed { id } => {
                // TODO handle removed
                println!("image asset {} removed", id);
                continue;
            }
            _ => (),
        }
    }

    if updated.is_empty() {
        return;
    }

    gpu_images.updated_this_frame = true;

    for asset_id in updated.iter() {
        if let Some(bevy_image) = images.get(*asset_id) {
            let handle: AssetId<Image> = asset_id.clone();
            if bevy_image.data.is_none() {
                continue;
            }
            let texture = unsafe {
                let texture = ctx.gl.create_texture().unwrap();
                ctx.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                // TODO actually set correct params/format/mips
                ctx.gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::LINEAR as i32,
                );
                ctx.gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::LINEAR as i32,
                );
                let size = bevy_image.size();
                ctx.gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA as i32,
                    size.x as i32,
                    size.y as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    PixelUnpackData::Slice(bevy_image.data.as_deref()),
                );
                texture
            };
            if let Some(index) = gpu_images.mapping.get(&handle).copied() {
                gpu_images.images[index as usize] = texture;
            } else {
                let index = gpu_images.images.len();
                gpu_images.mapping.insert(handle, index as u32);
                gpu_images.images.push(texture);
            }
        }
    }
}

use std::sync::Arc;

use bevy::{
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::render_resource::TextureFormat,
};

use glow::{HasContext, PixelUnpackData};
use wgpu_types::TextureViewDimension;

use crate::{BevyGlContext, render::RenderSet};

/// Handles uploading bevy Image assets to the GPU
pub struct PrepareImagePlugin;

#[derive(Resource, Deref)]
pub struct DefaultSampler(ImageSamplerDescriptor);

impl Plugin for PrepareImagePlugin {
    fn build(&self, app: &mut App) {
        if let Some(image_plugin) = app.get_added_plugins::<ImagePlugin>().first() {
            let default_sampler = image_plugin.default_sampler.clone();
            app.insert_resource(DefaultSampler(default_sampler));
        } else {
            warn!("No ImagePlugin found. Try adding PrepareImagePlugin after DefaultPlugins");
        }

        app.init_non_send_resource::<GpuImages>()
            .add_systems(PostUpdate, send_images_to_gpu.in_set(RenderSet::Prepare));
    }
}

#[derive(Default)]
pub struct GpuImages {
    // u32 is target glow::TEXTURE_2D or glow::TEXTURE_CUBE_MAP
    pub mapping: HashMap<AssetId<Image>, (glow::Texture, u32)>,
    pub updated_this_frame: bool,
    pub placeholder: Option<glow::Texture>,
    pub gl: Option<Arc<glow::Context>>,
}

impl Drop for GpuImages {
    fn drop(&mut self) {
        unsafe {
            for texture in self.mapping.values() {
                self.gl.as_ref().unwrap().delete_texture(texture.0);
            }
        }
    }
}

pub fn send_images_to_gpu(
    mut gpu_images: NonSendMut<GpuImages>,
    images: Res<Assets<Image>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    ctx: If<NonSend<BevyGlContext>>,
    default_sampler: Res<DefaultSampler>,
) {
    if gpu_images.gl.is_none() {
        gpu_images.gl = Some(ctx.gl.clone());
    }
    gpu_images.updated_this_frame = false;

    let mut updated: HashSet<AssetId<Image>> = HashSet::new();
    for event in image_events.read() {
        match event {
            AssetEvent::Modified { id } | AssetEvent::Added { id } => {
                updated.insert(id.clone());
            }
            AssetEvent::Removed { id } => {
                if let Some(tex) = gpu_images.mapping.remove(id) {
                    unsafe { ctx.gl.delete_texture(tex.0) };
                }
                continue;
            }
            _ => (),
        }
    }

    if updated.is_empty() {
        return;
    }

    if gpu_images.placeholder.is_none() {
        unsafe {
            let texture = ctx.gl.create_texture().unwrap();
            ctx.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            ctx.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                1,
                1,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                PixelUnpackData::Slice(Some(&[255, 255, 255, 255])),
            );
            gpu_images.placeholder = Some(texture);
        }
    }

    gpu_images.updated_this_frame = true;

    for asset_id in updated.iter() {
        if let Some(bevy_image) = images.get(*asset_id) {
            let handle: AssetId<Image> = asset_id.clone();
            if bevy_image.data.is_none() {
                continue;
            }

            let Some((texture, target)) =
                bevy_image_to_gl_texture(&ctx, Some(default_sampler.0.clone()), bevy_image)
            else {
                continue;
            };

            if let Some(old) = gpu_images.mapping.insert(handle, (texture, target)) {
                unsafe { ctx.gl.delete_texture(old.0) };
            }
        }
    }
}

/// Returns texture handle and target
pub fn bevy_image_to_gl_texture(
    ctx: &BevyGlContext,
    default_sampler: Option<ImageSamplerDescriptor>,
    bevy_image: &Image,
) -> Option<(glow::Texture, u32)> {
    let Some(target) = get_dimension_target(bevy_image) else {
        return None;
    };
    unsafe {
        let texture = ctx.gl.create_texture().unwrap();

        ctx.gl.bind_texture(target, Some(texture));
        let mip_level_count = bevy_image.texture_descriptor.mip_level_count;
        let sampler = match &bevy_image.sampler {
            ImageSampler::Default => default_sampler.unwrap_or(ImageSamplerDescriptor::linear()),
            ImageSampler::Descriptor(s) => s.clone(),
        };

        let min_filter = match &sampler.min_filter {
            ImageFilterMode::Nearest => {
                if mip_level_count > 1 {
                    glow::NEAREST_MIPMAP_NEAREST as i32
                } else {
                    glow::NEAREST as i32
                }
            }
            ImageFilterMode::Linear => {
                if mip_level_count > 1 {
                    glow::LINEAR_MIPMAP_LINEAR as i32
                } else {
                    glow::LINEAR as i32
                }
            }
        };

        let mag_filter = match &sampler.mag_filter {
            ImageFilterMode::Nearest => glow::NEAREST as i32,
            ImageFilterMode::Linear => glow::LINEAR as i32,
        };

        ctx.gl
            .tex_parameter_i32(target, glow::TEXTURE_MIN_FILTER, min_filter);
        ctx.gl
            .tex_parameter_i32(target, glow::TEXTURE_MAG_FILTER, mag_filter);

        if target == glow::TEXTURE_CUBE_MAP && !ctx.has_cube_map_seamless {
            let c2e = glow::CLAMP_TO_EDGE as i32;
            ctx.gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_S, c2e);
            ctx.gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_T, c2e);
            ctx.gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_R, c2e);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            ctx.gl
                .tex_parameter_i32(target, glow::TEXTURE_BASE_LEVEL, 0);
            ctx.gl.tex_parameter_i32(
                target,
                glow::TEXTURE_MAX_LEVEL,
                (mip_level_count - 1) as i32,
            );
        }

        transfer_image_data(bevy_image, target, ctx);
        // TODO make configurable
        set_anisotropy(&ctx.gl, target, 16);
        Some((texture, target))
    }
}

fn get_dimension_target(image: &Image) -> Option<u32> {
    let view = image.texture_view_descriptor.clone().unwrap_or_default();
    let dimension = view.dimension.unwrap_or_default();
    let target = match dimension {
        TextureViewDimension::D1 => return None,
        TextureViewDimension::D2 => glow::TEXTURE_2D,
        TextureViewDimension::D2Array => return None,
        TextureViewDimension::Cube => glow::TEXTURE_CUBE_MAP,
        TextureViewDimension::CubeArray => return None,
        TextureViewDimension::D3 => return None,
    };
    Some(target)
}

fn transfer_image_data(image: &bevy::prelude::Image, target: u32, ctx: &BevyGlContext) {
    let dim = match image.texture_descriptor.dimension {
        wgpu_types::TextureDimension::D1 => 1,
        wgpu_types::TextureDimension::D2 => 2,
        wgpu_types::TextureDimension::D3 => 3,
    };
    let format = image.texture_descriptor.format;
    let mip_level_count = image.texture_descriptor.mip_level_count;
    let array_layer_count = image.texture_descriptor.array_layer_count();
    let block_size = format.block_copy_size(None).unwrap_or(4);
    let (block_width, block_height) = format.block_dimensions();

    let mut binary_offset = 0;

    let size3d = (
        image.texture_descriptor.size.width,
        image.texture_descriptor.size.height,
        image.texture_descriptor.size.depth_or_array_layers,
    );

    let cube_targets = [
        glow::TEXTURE_CUBE_MAP_POSITIVE_X,
        glow::TEXTURE_CUBE_MAP_NEGATIVE_X,
        glow::TEXTURE_CUBE_MAP_POSITIVE_Y,
        glow::TEXTURE_CUBE_MAP_NEGATIVE_Y,
        glow::TEXTURE_CUBE_MAP_POSITIVE_Z,
        glow::TEXTURE_CUBE_MAP_NEGATIVE_Z,
    ];

    #[cfg(not(target_arch = "wasm32"))]
    let rgb_format = glow::RGBA8;
    #[cfg(target_arch = "wasm32")]
    let rgb_format = glow::RGBA;

    let internal_format = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm => rgb_format,
        TextureFormat::Rgba8UnormSrgb => rgb_format,
        #[cfg(not(target_arch = "wasm32"))]
        TextureFormat::Rgb9e5Ufloat => glow::RGB9_E5,
        #[cfg(target_arch = "wasm32")]
        TextureFormat::Rgb9e5Ufloat => rgb_format, // Not supported by WebGL1 so we convert to RGBE
        _ => {
            warn!("unimplemented format {:?}", image.texture_descriptor.format);
            return;
        }
    };

    let pixel_format = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm => glow::RGBA,
        TextureFormat::Rgba8UnormSrgb => glow::RGBA,
        #[cfg(not(target_arch = "wasm32"))]
        TextureFormat::Rgb9e5Ufloat => glow::RGB,
        #[cfg(target_arch = "wasm32")]
        TextureFormat::Rgb9e5Ufloat => glow::RGBA,
        _ => {
            warn!("unimplemented format {:?}", image.texture_descriptor.format);
            return;
        }
    };

    let pixel_type = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm => glow::UNSIGNED_BYTE,
        TextureFormat::Rgba8UnormSrgb => glow::UNSIGNED_BYTE,
        #[cfg(not(target_arch = "wasm32"))]
        TextureFormat::Rgb9e5Ufloat => glow::UNSIGNED_INT_5_9_9_9_REV,
        #[cfg(target_arch = "wasm32")]
        TextureFormat::Rgb9e5Ufloat => glow::UNSIGNED_BYTE,
        _ => {
            warn!("unimplemented format {:?}", image.texture_descriptor.format);
            return;
        }
    };

    let Some(image_data) = &image.data else {
        return;
    };

    #[allow(unused)]
    let mut converted_rgbe: Option<Vec<u32>> = None;
    #[cfg(target_arch = "wasm32")]
    if image.texture_descriptor.format == TextureFormat::Rgb9e5Ufloat {
        // rgb9e5 Not supported by WebGL1 so we convert to RGBE
        use shared_exponent_formats::rgb9e5::rgb9e5_to_vec3;
        converted_rgbe = Some(
            bytemuck::cast_slice::<u8, u32>(image_data)
                .iter()
                .map(|c| float2rgbe(rgb9e5_to_vec3(*c)))
                .collect::<Vec<u32>>(),
        );
    }

    let image_data = if let Some(converted_rgbe) = &converted_rgbe {
        bytemuck::cast_slice::<u32, u8>(converted_rgbe)
    } else {
        image_data
    };

    // https://github.com/gfx-rs/wgpu/blob/17fcb194258b05205d21001e8473762141ebda26/wgpu/src/util/device.rs#L15
    for mip_level in 0..mip_level_count as usize {
        if mip_level > 0 {
            #[cfg(target_arch = "wasm32")]
            unsafe {
                // TODO wasm seems to have issues when the mips are manually set.
                // Here we just do the first and let the driver generate the rest.
                // This may have unexpected results if the user was putting different data in each mip.
                ctx.gl.generate_mipmap(target);
                return;
            }
        }
        for array_layer in 0..array_layer_count {
            // https://github.com/bevyengine/bevy/blob/160bcc787c9b2f8dacafbf9dca7d7a6b2349386a/crates/bevy_render/src/texture/dds.rs#L318
            let mip_size = mip_level_size(size3d, mip_level, dim);
            // When uploading mips of compressed textures and the mip is supposed to be
            // a size that isn't a multiple of the block size, the mip needs to be uploaded
            // as its "physical size" which is the size rounded up to the nearest block size.
            let mip_physical = physical_size(mip_size, format);

            // All these calculations are performed on the physical size as that's the
            // data that exists in the buffer.
            let width_blocks = mip_physical.0 / block_width;
            let height_blocks = mip_physical.1 / block_height;

            let bytes_per_row = width_blocks * block_size;

            // TODO: this also had `* mip_size.depth;` but this seemed incorrect with multilayer which seemed layer major
            let data_size = bytes_per_row * height_blocks;

            let end_offset = binary_offset + data_size as usize;

            // https://github.com/gfx-rs/wgpu/blob/6f16ea460ab437173e14d2f5f3584ca7e1c9841d/wgpu-hal/src/vulkan/command.rs#L24
            let block_size = image
                .texture_descriptor
                .format
                .block_copy_size(Some(bevy::render::render_resource::TextureAspect::All))
                .unwrap();
            let _buffer_row_length = block_width * (bytes_per_row / block_size);

            if target != glow::TEXTURE_CUBE_MAP && array_layer != 0 {
                binary_offset = end_offset;
                continue;
            }
            // Only the first array layer is supported
            unsafe {
                ctx.gl.tex_image_2d(
                    if target == glow::TEXTURE_CUBE_MAP {
                        cube_targets[array_layer as usize]
                    } else {
                        glow::TEXTURE_2D
                    },
                    mip_level as i32,
                    internal_format as i32,
                    mip_size.0 as i32,
                    mip_size.1 as i32,
                    0,
                    pixel_format,
                    pixel_type,
                    PixelUnpackData::Slice(Some(&image_data[binary_offset..end_offset])),
                );
            };

            binary_offset = end_offset;
        }
    }
}

/// Calculates the extent at a given mip level.
/// Does *not* account for memory size being a multiple of block size.
///
/// <https://gpuweb.github.io/gpuweb/#logical-miplevel-specific-texture-extent>
pub fn mip_level_size(extent: (u32, u32, u32), level: usize, dim: usize) -> (u32, u32, u32) {
    // https://github.com/gfx-rs/wgpu/blob/6f16ea460ab437173e14d2f5f3584ca7e1c9841d/wgpu-types/src/lib.rs#L5779

    (
        u32::max(1, extent.0 >> level),
        match dim {
            1 => 1,
            _ => u32::max(1, extent.1 >> level),
        },
        match dim {
            1 => 1,
            2 => extent.2,
            3 => u32::max(1, extent.2 >> level),
            _ => 1,
        },
    )
}

/// Calculates the [physical size] backing a texture of the given
/// format and extent.  This includes padding to the block width
/// and height of the format.
///
/// This is the texture extent that you must upload at when uploading to _mipmaps_ of compressed textures.
///
/// [physical size]: https://gpuweb.github.io/gpuweb/#physical-miplevel-specific-texture-extent
pub fn physical_size(extent: (u32, u32, u32), format: TextureFormat) -> (u32, u32, u32) {
    // https://github.com/gfx-rs/wgpu/blob/6f16ea460ab437173e14d2f5f3584ca7e1c9841d/wgpu-types/src/lib.rs#L5744
    let (block_width, block_height) = format.block_dimensions();

    let width = ((extent.0 + block_width - 1) / block_width) * block_width;
    let height = ((extent.1 + block_height - 1) / block_height) * block_height;

    (width, height, extent.2)
}

fn set_anisotropy(gl: &glow::Context, target: u32, requested: u32) {
    unsafe {
        let ext = gl.supported_extensions();
        let supported = ext.contains("GL_EXT_texture_filter_anisotropic")
            || ext.contains("EXT_texture_filter_anisotropic");
        if supported {
            let max = gl.get_parameter_f32(glow::MAX_TEXTURE_MAX_ANISOTROPY_EXT);
            gl.tex_parameter_f32(
                target,
                glow::TEXTURE_MAX_ANISOTROPY_EXT,
                (requested as f32).clamp(1.0, max),
            );
        }
    }
}

pub fn frexpf(x: f32) -> (f32, i32) {
    let mut y = x.to_bits();
    let ee: i32 = ((y >> 23) & 0xff) as i32;

    if ee == 0 {
        if x != 0.0 {
            let x1p64 = f32::from_bits(0x5f800000);
            let (x, e) = frexpf(x * x1p64);
            return (x, e - 64);
        } else {
            return (x, 0);
        }
    } else if ee == 0xff {
        return (x, 0);
    }

    let e = ee - 0x7e;
    y &= 0x807fffff;
    y |= 0x3f000000;
    (f32::from_bits(y), e)
}

// https://www.graphics.cornell.edu/~bjw/rgbe.html
#[allow(unused)]
fn float2rgbe(color: [f32; 3]) -> u32 {
    let vmax = color[0].max(color[1]).max(color[2]);
    if vmax < 1e-32 {
        0
    } else {
        let (m, e) = frexpf(vmax);
        let scale = m * 256.0 / vmax;
        u32::from_le_bytes([
            (color[0] * scale) as u8,
            (color[1] * scale) as u8,
            (color[2] * scale) as u8,
            (e + 128) as u8,
        ])
    }
}

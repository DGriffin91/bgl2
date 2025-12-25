use std::rc::Rc;

use bevy::{
    mesh::MeshVertexAttribute,
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use bytemuck::cast_slice;
use glow::{Context, HasContext};

use crate::{
    AttribType, BevyGlContext,
    mesh_util::{get_attribute_f32x3, get_mesh_indices_u16, get_mesh_indices_u32},
    render::RenderSet,
};

/// Handles uploading bevy mesh assets to the GPU
pub struct PrepareMeshPlugin;

impl Plugin for PrepareMeshPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send_resource::<GPUMeshBufferMap>()
            .add_systems(
                PostUpdate,
                (send_standard_meshes_to_gpu)
                    .chain()
                    .in_set(RenderSet::Prepare),
            );
    }
}

pub struct GpuMeshBufferSet {
    pub buffers: Vec<(MeshVertexAttribute, glow::Buffer)>,
    pub index: glow::Buffer,
    pub all_index_count: usize,
    pub index_element_type: u32,
}

impl GpuMeshBufferSet {
    fn delete(&self, gl: &Context) {
        unsafe {
            gl.delete_buffer(self.index);
            for (_, b) in &self.buffers {
                gl.delete_buffer(*b)
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct BufferRef {
    pub buffer_index: usize,
    pub indices_start: usize,
    pub indices_count: usize,
    pub index_element_type: u32,
}

#[derive(Default)]
pub struct GPUMeshBufferMap {
    pub buffers: Vec<Option<(GpuMeshBufferSet, HashSet<AssetId<Mesh>>)>>,
    pub map: HashMap<AssetId<Mesh>, BufferRef>,
    pub gl: Option<Rc<glow::Context>>,
}

impl Drop for GPUMeshBufferMap {
    fn drop(&mut self) {
        for buffer in &self.buffers {
            if let Some((buffer, _)) = buffer {
                buffer.delete(self.gl.as_ref().unwrap());
            }
        }
    }
}

impl GPUMeshBufferMap {
    pub fn bind(
        &mut self,
        ctx: &BevyGlContext,
        mesh: &AssetId<Mesh>,
        shader_index: u32,
    ) -> Option<BufferRef> {
        if let Some(buffer_ref) = self.map.get(mesh) {
            if let Some((buffers, _)) = &self.buffers[buffer_ref.buffer_index] {
                unsafe {
                    ctx.gl
                        .bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(buffers.index));
                };
                for (att, buffer) in &buffers.buffers {
                    // TODO use caching to avoid looking up from the name here
                    if let Some(loc) = ctx.get_attrib_location(shader_index, att.name) {
                        let attrib_type = AttribType::from_bevy_vertex_format(att.format);
                        ctx.bind_vertex_attrib(
                            loc,
                            att.format.size() as u32 / attrib_type.gl_type_bytes(),
                            attrib_type,
                            *buffer,
                        );
                    }
                }
                return Some(*buffer_ref);
            }
        }
        None
    }
}

pub fn send_standard_meshes_to_gpu(
    meshes: Res<Assets<Mesh>>,
    mut gpu_meshes: NonSendMut<GPUMeshBufferMap>,
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
    mut index_buffer_data_u16: Local<Vec<u16>>,
    mut index_buffer_data_u32: Local<Vec<u32>>,
    ctx: If<NonSend<BevyGlContext>>,
) {
    if gpu_meshes.gl.is_none() {
        gpu_meshes.gl = Some(ctx.gl.clone());
    }

    for event in mesh_events.read() {
        let mesh_h = match event {
            AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Added { id }
            | AssetEvent::Modified { id } => id,
            AssetEvent::Removed { id } => {
                if let Some(buffer_ref) = gpu_meshes.map.remove(id) {
                    // after removing mapping, also remove it from the old set
                    // If the old set now has zero references, remove the buffer.
                    let mut buffer_unused = false;
                    if let Some((_old_buffer, set)) =
                        &mut gpu_meshes.buffers[buffer_ref.buffer_index]
                    {
                        set.remove(id);
                        buffer_unused = set.is_empty();
                    }
                    if buffer_unused {
                        if let Some((old_buffer, _)) =
                            gpu_meshes.buffers[buffer_ref.buffer_index].take()
                        {
                            old_buffer.delete(&ctx.gl);
                        }
                    }
                }
                continue;
            }
            AssetEvent::Unused { id: _ } => continue,
        };
        let Some(mesh) = meshes.get(*mesh_h) else {
            continue;
        };
        index_buffer_data_u16.clear();
        index_buffer_data_u32.clear();

        let positions = get_attribute_f32x3(mesh, Mesh::ATTRIBUTE_POSITION)
            .expect("Meshes vertex positions are required");

        let vertex_count = positions.len();
        let index_count;

        let (index_buffer, element_type) = if vertex_count >= u16::MAX as usize {
            let es_or_webgl = unsafe {
                ctx.gl
                    .get_parameter_string(glow::SHADING_LANGUAGE_VERSION)
                    .contains(" ES ")
            };
            if es_or_webgl
                && !ctx
                    .gl
                    .supported_extensions()
                    .contains("OES_element_index_uint")
            {
                warn!(
                    "Too many vertices. Base OpenGL ES 2.0 and WebGL 1.0 with OES_element_index_uint only support GL_UNSIGNED_BYTE or GL_UNSIGNED_SHORT"
                );
                // Could split up mesh data and then issue multiple calls, but if a platform doesn't have
                // OES_element_index_uint it might also struggle with so many tris.
                continue;
            }
            index_count = get_mesh_indices_u32(mesh, &mut index_buffer_data_u32);
            (
                ctx.gen_vbo_element(cast_slice(&index_buffer_data_u32), glow::STATIC_DRAW),
                glow::UNSIGNED_INT,
            )
        } else {
            index_count = get_mesh_indices_u16(mesh, &mut index_buffer_data_u16);
            (
                ctx.gen_vbo_element(cast_slice(&index_buffer_data_u16), glow::STATIC_DRAW),
                glow::UNSIGNED_SHORT,
            )
        };

        let buffers = mesh
            .attributes()
            .map(|(mesh_attribute, data)| {
                // TODO convert unsupported data types (like f16 to f32)
                (
                    *mesh_attribute,
                    ctx.gen_vbo(data.get_bytes(), glow::STATIC_DRAW),
                )
            })
            .collect();

        let buffer_index = gpu_meshes.buffers.len();
        gpu_meshes.buffers.push(Some((
            GpuMeshBufferSet {
                buffers,
                all_index_count: index_count,
                index: index_buffer,
                index_element_type: element_type,
            },
            HashSet::from_iter([mesh_h.clone()]),
        )));

        let buffer_ref = BufferRef {
            buffer_index,
            indices_start: 0,
            indices_count: index_count,
            index_element_type: element_type,
        };

        // Add mapping from mesh handle to buffer. If this handle already had a mapping, remove it from the old set.
        // If the old set now has zero references, remove the buffer.
        if let Some(old_buffer_ref) = gpu_meshes.map.insert(mesh_h.clone(), buffer_ref) {
            let mut buffer_unused = false;
            if let Some((_old_buffer, set)) = &mut gpu_meshes.buffers[old_buffer_ref.buffer_index] {
                set.remove(mesh_h);
                buffer_unused = set.is_empty();
            }
            if buffer_unused {
                if let Some((old_buffer, _)) =
                    gpu_meshes.buffers[old_buffer_ref.buffer_index].take()
                {
                    old_buffer.delete(&ctx.gl);
                }
            }
        }
    }
}

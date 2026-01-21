use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use bytemuck::cast_slice;
use glow::HasContext;
use std::hash::Hash;
use std::hash::Hasher;
use wgpu_types::VertexFormat;

use crate::{
    AttribType, BevyGlContext, BufferRef, GpuMeshBufferSet, ShaderIndex,
    command_encoder::CommandEncoder,
    mesh_util::{get_attribute_f32x3, get_mesh_indices_u16, get_mesh_indices_u32},
    render::RenderSet,
};

/// Handles uploading bevy mesh assets to the GPU
pub struct PrepareMeshPlugin;

impl Plugin for PrepareMeshPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (send_standard_meshes_to_gpu)
                .chain()
                .in_set(RenderSet::Prepare),
        );
    }
}

#[derive(Default)]
pub struct GPUMeshBufferMap {
    pub last_bind: Option<(ShaderIndex, usize)>, //shader_index, buffer_index
    pub buffers: Vec<Option<(GpuMeshBufferSet, HashSet<AssetId<Mesh>>)>>,
    pub map: HashMap<AssetId<Mesh>, BufferRef>,
}

impl BevyGlContext {
    /// Call before using bind() or draw_mesh()
    pub fn reset_mesh_bind_cache(&mut self) {
        self.mesh.last_bind = None;
    }

    /// Make sure to call reset_bind_cache() before the first iteration of bind(). It doesn't know about whatever random
    /// opengl state came before.
    pub fn bind_mesh(&mut self, mesh: &AssetId<Mesh>, shader_index: u32) -> Option<BufferRef> {
        if let Some(buffer_ref) = self.mesh.map.get(mesh) {
            if let Some((buffers, _)) = &self.mesh.buffers[buffer_ref.buffer_index] {
                let this_bind_set = Some((shader_index, buffer_ref.buffer_index));
                if this_bind_set == self.mesh.last_bind {
                    return Some(*buffer_ref);
                }
                self.mesh.last_bind = this_bind_set;
                unsafe {
                    self.gl
                        .bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(buffers.index));
                };
                for (att, buffer) in &buffers.buffers {
                    // TODO use caching to avoid looking up from the name here
                    if let Some(loc) = self.get_attrib_location(shader_index, att.name) {
                        let attrib_type = AttribType::from_bevy_vertex_format(att.format);
                        self.bind_vertex_attrib(
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

    /// Make sure to call reset_bind_cache() before the first iteration of bind(). It doesn't know about whatever random
    /// opengl state came before.
    pub fn draw_mesh(&mut self, mesh: AssetId<Mesh>, shader_index: u32) {
        // Extremely slow temporary workaround for initially testing macos
        #[cfg(target_os = "macos")]
        self.reset_bind_cache();
        #[cfg(target_os = "macos")]
        let vao = unsafe {
            let vao = ctx.gl.create_vertex_array().unwrap();
            ctx.gl.bind_vertex_array(Some(vao));
            vao
        };
        if let Some(buffer_ref) = self.bind_mesh(&mesh, shader_index) {
            unsafe {
                self.gl.draw_elements(
                    glow::TRIANGLES,
                    buffer_ref.indices_count as i32,
                    buffer_ref.index_element_type,
                    buffer_ref.bytes_offset,
                );
            };
        }
        #[cfg(target_os = "macos")]
        unsafe {
            ctx.gl.bind_vertex_array(None);
            ctx.gl.delete_vertex_array(vao);
        }
    }
}

pub fn send_standard_meshes_to_gpu(
    bevy_meshes: Res<Assets<Mesh>>,
    //mut gpu_meshes: NonSendMut<GPUMeshBufferMap>,
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
    mut cmd: ResMut<CommandEncoder>,
) {
    // key is hash of vertex attribute props
    let mut meshes_by_attr: HashMap<u64, Vec<AssetId<Mesh>>> = HashMap::new();
    let mut meshes = HashMap::new();

    for event in mesh_events.read() {
        let mesh_h = match event {
            AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Added { id }
            | AssetEvent::Modified { id } => id,
            AssetEvent::Removed { id } => {
                let id = *id;
                cmd.record(move |ctx: &mut BevyGlContext| {
                    if let Some(buffer_ref) = ctx.mesh.map.remove(&id) {
                        // after removing mapping, also remove it from the old set
                        // If the old set now has zero references, remove the buffer.
                        let mut buffer_unused = false;
                        if let Some((_old_buffer, set)) =
                            &mut ctx.mesh.buffers[buffer_ref.buffer_index]
                        {
                            set.remove(&id);
                            buffer_unused = set.is_empty();
                        }
                        if buffer_unused {
                            if let Some((old_buffer, _)) =
                                ctx.mesh.buffers[buffer_ref.buffer_index].take()
                            {
                                old_buffer.delete(&ctx.gl);
                            }
                        }
                    }
                });
                continue;
            }
            AssetEvent::Unused { id: _ } => continue,
        };

        let Some(mesh) = bevy_meshes.get(*mesh_h) else {
            continue;
        };

        meshes.insert(*mesh_h, mesh.clone());

        let mut hasher = std::hash::DefaultHasher::new();

        let attributes = mesh.attributes();

        for (a, _) in attributes {
            a.id.hash(&mut hasher);
            a.format.hash(&mut hasher);
        }
        let attr_hash = hasher.finish();

        // See if there's other meshes that were added this frame that this one could be packed with.
        if let Some(mesh_h_set) = meshes_by_attr.get_mut(&attr_hash) {
            mesh_h_set.push(*mesh_h);
        } else {
            meshes_by_attr.insert(attr_hash, vec![*mesh_h]);
        }
    }

    let mut meshes_by_attr = meshes_by_attr;
    cmd.record(move |ctx: &mut BevyGlContext| {
        // TODO reuse allocations
        let mut index_buffer_data_u16 = Vec::new();
        let mut index_buffer_data_u32 = Vec::new();
        let mut scratch_floats = Vec::new();

        let es_or_webgl = unsafe {
            ctx.gl
                .get_parameter_string(glow::SHADING_LANGUAGE_VERSION)
                .contains(" ES ")
        };
        let u16_indices = es_or_webgl
            && !ctx
                .gl
                .supported_extensions()
                .contains("OES_element_index_uint");
        let element_type = if u16_indices {
            glow::UNSIGNED_SHORT
        } else {
            glow::UNSIGNED_INT
        };
        let max_verts_per_buffer = if u16_indices {
            u16::MAX as usize
        } else {
            u32::MAX as usize
        };

        // Groups of meshes to be combined.
        let mut mesh_groups: Vec<Vec<AssetId<Mesh>>> = Vec::new();

        // Go though meshes_by_attr and create groups that can fit in the index space available (which might only be u16::MAX)
        for (_, mesh_handles) in meshes_by_attr.drain() {
            let mut mesh_group = Vec::new();
            let mut accum_positions = 0;
            let mut accum_indices = 0;
            for mesh_h in mesh_handles {
                let Some(mesh) = meshes.get(&mesh_h) else {
                    continue;
                };
                let positions_count = get_attribute_f32x3(mesh, Mesh::ATTRIBUTE_POSITION)
                    .expect("Meshes vertex positions are required")
                    .len();
                accum_positions += positions_count;
                accum_indices += mesh.indices().map_or(positions_count, |ind| ind.len());
                // The math for accum_indices is because draw_elements offset is an i32 that uses bytes. Doesn't matter that
                // i16 would only be 2 bytes since if this was over it would also easily already be over for u16 in general.
                if accum_positions < max_verts_per_buffer && accum_indices * 4 < i32::MAX as usize {
                    // If a single mesh goes over, it ends up being skipped here. TODO break into multiple meshes.
                    mesh_group.push(mesh_h);
                } else {
                    accum_positions = 0;
                    accum_indices = 0;
                    let mut new_group = Vec::new();
                    std::mem::swap(&mut mesh_group, &mut new_group);
                    mesh_groups.push(new_group);
                }
            }
            if !mesh_group.is_empty() {
                mesh_groups.push(mesh_group);
            }
        }

        // For each group of matching meshes, collect the vertex attributes and offset indices
        for mesh_handles in mesh_groups {
            let next_buffer_set_index = ctx.mesh.buffers.len();
            index_buffer_data_u16.clear();
            index_buffer_data_u32.clear();

            let Some(first_mesh_h) = mesh_handles.get(0) else {
                continue;
            };
            let Some(first_mesh) = meshes.get(first_mesh_h) else {
                continue;
            };

            let count = first_mesh.attributes().count();

            let mut buffer_data: Vec<Vec<u8>> = vec![Vec::new(); count];

            let mut vertex_offset = 0;
            let mut index_offset = 0;
            for mesh_h in &mesh_handles {
                let Some(mesh) = meshes.get(mesh_h) else {
                    continue;
                };

                let positions = get_attribute_f32x3(mesh, Mesh::ATTRIBUTE_POSITION)
                    .expect("Meshes vertex positions are required");

                let vertex_count = positions.len();

                let index_count = if u16_indices {
                    if (vertex_count + vertex_offset) >= u16::MAX as usize {
                        warn!(
                            "Too many vertices. Base OpenGL ES 2.0 and WebGL 1.0 with OES_element_index_uint only support GL_UNSIGNED_BYTE or GL_UNSIGNED_SHORT"
                        );
                        // Could split up mesh data and then issue multiple calls, but if a platform doesn't have
                        // OES_element_index_uint it might also struggle with so many tris.
                        continue;
                    }
                    get_mesh_indices_u16(mesh, &mut index_buffer_data_u16, vertex_offset as u16)
                } else {
                    get_mesh_indices_u32(mesh, &mut index_buffer_data_u32, vertex_offset as u32)
                };

                mesh.attributes()
                    .zip(buffer_data.iter_mut())
                    .for_each(|((_, data), dst_data)| {
                        // TODO convert unsupported data types (like f16 to f32)
                        dst_data.extend(data.get_bytes());
                    });

                let buffer_ref = BufferRef {
                    buffer_index: next_buffer_set_index,
                    indices_start: index_offset,
                    indices_count: index_count,
                    index_element_type: element_type,
                    bytes_offset: index_offset as i32 * if u16_indices { 2 } else { 4 },
                };

                // Add mapping from mesh handle to buffer. If this handle already had a mapping, remove it from the old set.
                // If the old set now has zero references, remove the buffer.
                if let Some(old_buffer_ref) = ctx.mesh.map.insert(mesh_h.clone(), buffer_ref) {
                    let mut buffer_unused = false;
                    if let Some(b) = ctx.mesh.buffers.get_mut(old_buffer_ref.buffer_index) {
                        if let Some((_old_buffer, set)) = b {
                            set.remove(mesh_h);
                            buffer_unused = set.is_empty();
                        }
                    }
                    if buffer_unused {
                        if let Some((old_buffer, _)) =
                            ctx.mesh.buffers[old_buffer_ref.buffer_index].take()
                        {
                            old_buffer.delete(&ctx.gl);
                        }
                    }
                }

                index_offset += index_count;
                vertex_offset += vertex_count;
            }

            // Create combined GPU index buffer
            let index_buffer = ctx.gen_vbo_element(
                if u16_indices {
                    cast_slice(&index_buffer_data_u16)
                } else {
                    cast_slice(&index_buffer_data_u32)
                },
                glow::STATIC_DRAW,
            );

            // Create combined vertex attribute buffers
            let buffers = first_mesh
                .attributes()
                .zip(buffer_data.iter_mut())
                .map(|((mesh_attribute, _), data)| {
                    let mut mesh_attribute = *mesh_attribute;
                    let converted_data = match mesh_attribute.format {
                        // Vertex_JointIndex uses Uint16x4 but this type is not supported so Float32x4 is used instead
                        VertexFormat::Uint16x4 => {
                            scratch_floats.clear();
                            scratch_floats
                                .extend(cast_slice::<u8, u16>(data).iter().map(|v| *v as f32));
                            mesh_attribute.format = VertexFormat::Float32x4;
                            cast_slice::<f32, u8>(&scratch_floats)
                        }
                        _ => data,
                    };

                    (
                        mesh_attribute,
                        ctx.gen_vbo(converted_data, glow::STATIC_DRAW),
                    )
                })
                .collect();

            ctx.mesh.buffers.push(Some((
                GpuMeshBufferSet {
                    buffers,
                    index: index_buffer,
                    index_element_type: element_type,
                },
                HashSet::from_iter(mesh_handles),
            )));
        }
    });
}

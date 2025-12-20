use bevy::{platform::collections::HashMap, prelude::*};
use bytemuck::{Pod, Zeroable, cast_slice};
use glow::HasContext;

use crate::{
    BevyGlContext,
    mesh_util::{get_attribute_f32x2, get_attribute_f32x3, get_attribute_f32x4, get_mesh_indices},
};

pub struct GpuMeshBuffers {
    pub position: glow::Buffer,
    pub index: glow::Buffer,
    pub normal: glow::Buffer,
    pub index_count: usize,
}

#[derive(Resource, Default)]
pub struct GPUMeshBufferMap {
    pub buffers: HashMap<AssetId<Mesh>, GpuMeshBuffers>, // TODO delete old and overwritten
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct VertexData {
    pub color: Vec4,
    pub tangent: Vec4,
    pub position: Vec4, // TODO use hlsl or C layout
    pub normal: Vec4,   // TODO use hlsl or C layout
    pub uv_0: Vec2,
    pub uv_1: Vec2,
}

pub fn send_standard_meshes_to_gpu(
    meshes: Res<Assets<Mesh>>,
    mut gpu_meshes: ResMut<GPUMeshBufferMap>,
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
    mut index_buffer_data: Local<Vec<u16>>,
    mut vertex_data: Local<Vec<VertexData>>,
    ctx: If<NonSend<BevyGlContext>>,
) {
    for event in mesh_events.read() {
        let mesh_h = match event {
            AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Added { id }
            | AssetEvent::Modified { id } => id,
            AssetEvent::Removed { id } => {
                let _ = gpu_meshes.buffers.remove(id);
                dbg!("Need to impl delete and overwrite");
                continue;
            }
            AssetEvent::Unused { id: _ } => continue,
        };
        let Some(mesh) = meshes.get(*mesh_h) else {
            continue;
        };
        index_buffer_data.clear();
        vertex_data.clear();

        let positions = get_attribute_f32x3(mesh, Mesh::ATTRIBUTE_POSITION)
            .expect("Meshes vertex positions are required");
        let normals = get_attribute_f32x3(mesh, Mesh::ATTRIBUTE_NORMAL)
            .expect("Meshes vertex normals are required");

        let vertex_count = positions.len();

        let mut empty_tangents = Vec::new();
        let mut empty_uv0 = Vec::new();
        let mut empty_uv1 = Vec::new();
        let mut empty_colors = Vec::new();

        let _tangents = get_attribute_f32x4(mesh, Mesh::ATTRIBUTE_TANGENT).unwrap_or_else(|| {
            empty_tangents.resize(vertex_count, [f32::INFINITY; 4]);
            &empty_tangents
        });
        let _uv_0 = get_attribute_f32x2(mesh, Mesh::ATTRIBUTE_UV_0).unwrap_or_else(|| {
            empty_uv0.resize(vertex_count, [0.0; 2]);
            &empty_uv0
        });
        let _uv_1 = get_attribute_f32x2(mesh, Mesh::ATTRIBUTE_UV_1).unwrap_or_else(|| {
            empty_uv1.resize(vertex_count, [0.0; 2]);
            &empty_uv1
        });
        let _colors = get_attribute_f32x4(mesh, Mesh::ATTRIBUTE_COLOR).unwrap_or_else(|| {
            empty_colors.resize(vertex_count, [1.0; 4]);
            &empty_colors
        });

        if vertex_count >= u16::MAX as usize {
            panic!(
                "Too many vertices. Base OpenGL ES 2.0 and WebGL 1.0 only support GL_UNSIGNED_BYTE or GL_UNSIGNED_SHORT"
            )
        }

        let index_count = if let Some(index_count) = get_mesh_indices(mesh, &mut index_buffer_data)
        {
            index_count
        } else {
            index_buffer_data.append(&mut (0..vertex_count as u16).map(|i| i).collect::<Vec<_>>());
            vertex_count
        };

        //vertex_data.extend((0..vertex_count).map(|i| VertexData {
        //    position: Into::<Vec3>::into(positions[i]).extend(0.0),
        //    normal: Into::<Vec3>::into(normals[i]).extend(0.0),
        //    uv_0: uv_0[i].into(),
        //    uv_1: uv_1[i].into(),
        //    color: colors[i].into(),
        //    tangent: tangents[i].into(),
        //}));

        unsafe {
            // copy our vertices array in a vertex buffer for OpenGL to use
            let position_vbo = ctx.gl.create_buffer().unwrap();
            ctx.gl.bind_buffer(glow::ARRAY_BUFFER, Some(position_vbo));
            ctx.gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_slice(&positions),
                glow::STATIC_DRAW,
            );
            ctx.gl.bind_buffer(glow::ARRAY_BUFFER, None);

            // copy our index array in a element buffer for OpenGL to use
            let index_vbo = ctx.gl.create_buffer().unwrap();
            ctx.gl
                .bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(index_vbo));
            ctx.gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                cast_slice(&index_buffer_data),
                glow::STATIC_DRAW,
            );
            ctx.gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);

            // Normals
            let normal_vbo = ctx.gl.create_buffer().unwrap();
            ctx.gl.bind_buffer(glow::ARRAY_BUFFER, Some(normal_vbo));
            ctx.gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_slice(&normals),
                glow::STATIC_DRAW,
            );
            ctx.gl.bind_buffer(glow::ARRAY_BUFFER, None);

            gpu_meshes.buffers.insert(
                mesh_h.clone(),
                GpuMeshBuffers {
                    index_count: index_count,
                    position: position_vbo,
                    index: index_vbo,
                    normal: normal_vbo,
                },
            );
        }
    }
}

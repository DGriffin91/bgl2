use bevy::{platform::collections::HashMap, prelude::*};
use bytemuck::cast_slice;
use glow::{Context, HasContext};

use crate::{
    BevyGlContext,
    mesh_util::{get_attribute_f32x2, get_attribute_f32x3, get_attribute_f32x4, get_mesh_indices},
    render::RenderSet,
};

/// Handles uploading bevy mesh assets to the GPU
pub struct PrepareMeshPlugin;

impl Plugin for PrepareMeshPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GPUMeshBufferMap>().add_systems(
            PostUpdate,
            (send_standard_meshes_to_gpu)
                .chain()
                .in_set(RenderSet::Prepare),
        );
    }
}

pub struct GpuMeshBuffers {
    pub position: glow::Buffer,
    pub index: glow::Buffer,
    pub normal: Option<glow::Buffer>,
    pub tangent: Option<glow::Buffer>,
    pub uv_0: Option<glow::Buffer>,
    pub uv_1: Option<glow::Buffer>,
    pub color: Option<glow::Buffer>,
    pub index_count: usize,
}

impl GpuMeshBuffers {
    fn delete(&self, gl: &Context) {
        unsafe {
            // TODO make reusable pattern. Can we access gl another way? If we do this on drop it would need to be
            // on the right thread?
            gl.delete_buffer(self.position);
            gl.delete_buffer(self.index);
            self.normal.map(|b| gl.delete_buffer(b));
            self.tangent.map(|b| gl.delete_buffer(b));
            self.uv_0.map(|b| gl.delete_buffer(b));
            self.color.map(|b| gl.delete_buffer(b));
        }
    }
}

#[derive(Resource, Default)]
pub struct GPUMeshBufferMap {
    pub buffers: HashMap<AssetId<Mesh>, GpuMeshBuffers>,
}

pub fn send_standard_meshes_to_gpu(
    meshes: Res<Assets<Mesh>>,
    mut gpu_meshes: ResMut<GPUMeshBufferMap>,
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
    mut index_buffer_data: Local<Vec<u16>>,
    ctx: If<NonSend<BevyGlContext>>,
) {
    for event in mesh_events.read() {
        let mesh_h = match event {
            AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Added { id }
            | AssetEvent::Modified { id } => id,
            AssetEvent::Removed { id } => {
                if let Some(buffers) = gpu_meshes.buffers.remove(id) {
                    buffers.delete(&ctx.gl);
                }
                continue;
            }
            AssetEvent::Unused { id: _ } => continue,
        };
        let Some(mesh) = meshes.get(*mesh_h) else {
            continue;
        };
        index_buffer_data.clear();

        let positions = get_attribute_f32x3(mesh, Mesh::ATTRIBUTE_POSITION)
            .expect("Meshes vertex positions are required");

        let vertex_count = positions.len();

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

        if let Some(old_buffer) = gpu_meshes.buffers.insert(
            mesh_h.clone(),
            GpuMeshBuffers {
                index_count: index_count,
                position: ctx.gen_vbo(cast_slice(&positions), glow::STATIC_DRAW),
                index: ctx.gen_vbo_element(cast_slice(&index_buffer_data), glow::STATIC_DRAW),
                normal: get_attribute_f32x3(mesh, Mesh::ATTRIBUTE_NORMAL)
                    .map(|data| ctx.gen_vbo(cast_slice(data), glow::STATIC_DRAW)),
                tangent: get_attribute_f32x4(mesh, Mesh::ATTRIBUTE_TANGENT)
                    .map(|data| ctx.gen_vbo(cast_slice(data), glow::STATIC_DRAW)),
                uv_0: get_attribute_f32x2(mesh, Mesh::ATTRIBUTE_UV_0)
                    .map(|data| ctx.gen_vbo(cast_slice(data), glow::STATIC_DRAW)),
                uv_1: get_attribute_f32x2(mesh, Mesh::ATTRIBUTE_UV_1)
                    .map(|data| ctx.gen_vbo(cast_slice(data), glow::STATIC_DRAW)),
                color: get_attribute_f32x4(mesh, Mesh::ATTRIBUTE_COLOR)
                    .map(|data| ctx.gen_vbo(cast_slice(data), glow::STATIC_DRAW)),
            },
        ) {
            old_buffer.delete(&ctx.gl);
        }
    }
}

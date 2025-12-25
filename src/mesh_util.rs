use bevy::{
    mesh::{Indices, MeshVertexAttributeId, VertexAttributeValues},
    prelude::*,
};

pub fn get_mesh_indices_u16(mesh: &Mesh, index_buffer_data: &mut Vec<u16>, offset: u16) -> usize {
    if let Some(indices) = mesh.indices() {
        match indices {
            Indices::U16(indices) => {
                indices.iter().for_each(|i| {
                    index_buffer_data.push(i + offset);
                });
            }
            Indices::U32(indices) => {
                indices.iter().for_each(|i| {
                    index_buffer_data.push(*i as u16 + offset);
                });
            }
        };
        indices.len()
    } else {
        let vertex_count = get_attribute_f32x3(mesh, Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .len();
        index_buffer_data.append(
            &mut (offset..vertex_count as u16 + offset)
                .map(|i| i)
                .collect::<Vec<_>>(),
        );
        vertex_count
    }
}

pub fn get_mesh_indices_u32(mesh: &Mesh, index_buffer_data: &mut Vec<u32>, offset: u32) -> usize {
    if let Some(indices) = mesh.indices() {
        match indices {
            Indices::U16(indices) => {
                indices.iter().for_each(|i| {
                    index_buffer_data.push(*i as u32 + offset);
                });
            }
            Indices::U32(indices) => {
                indices.iter().for_each(|i| {
                    index_buffer_data.push(i + offset);
                });
            }
        };
        indices.len()
    } else {
        let vertex_count = get_attribute_f32x3(mesh, Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .len();
        index_buffer_data.append(
            &mut (offset..vertex_count as u32 + offset)
                .map(|i| i)
                .collect::<Vec<_>>(),
        );
        vertex_count
    }
}

pub fn get_attribute_f32x2(
    mesh: &Mesh,
    id: impl Into<MeshVertexAttributeId>,
) -> Option<&[[f32; 2]]> {
    let Some(data) = mesh.attribute(id) else {
        return None;
    };
    let VertexAttributeValues::Float32x2(data) = data else {
        panic!("Invalid Vertex Attribute Format");
    };
    Some(data)
}

pub fn get_attribute_f32x3(
    mesh: &Mesh,
    id: impl Into<MeshVertexAttributeId>,
) -> Option<&[[f32; 3]]> {
    let Some(data) = mesh.attribute(id) else {
        return None;
    };
    let data = data.as_float3().unwrap();
    Some(data)
}

pub fn get_attribute_f32x4(
    mesh: &Mesh,
    id: impl Into<MeshVertexAttributeId>,
) -> Option<&Vec<[f32; 4]>> {
    let Some(data) = mesh.attribute(id) else {
        return None;
    };
    let VertexAttributeValues::Float32x4(data) = data else {
        panic!("Invalid Tangent Attribute Format");
    };
    Some(data)
}

// https://jcgt.org/published/0003/02/01/paper.pdf

/// Encodes normals or unit direction vectors as octahedral coordinates.
#[inline]
pub fn octahedral_encode(v: Vec3) -> Vec2 {
    let n = v / (v.x.abs() + v.y.abs() + v.z.abs());
    let octahedral_wrap = (1.0 - n.yx().abs()) * n.xy().signum();
    let n_xy = if n.z >= 0.0 { n.xy() } else { octahedral_wrap };
    n_xy * 0.5 + 0.5
}

/// Decodes normals or unit direction vectors from octahedral coordinates.
#[inline]
pub fn octahedral_decode(v: Vec2) -> Vec3 {
    let f = v * 2.0 - 1.0;
    let mut n = Vec3::new(f.x, f.y, 1.0 - f.x.abs() - f.y.abs());
    let t = (-n.z).max(0.0);
    let w = vec2(
        if n.x >= 0.0 { -t } else { t },
        if n.y >= 0.0 { -t } else { t },
    );
    n = Vec3::new(n.x + w.x, n.y + w.y, n.z);
    n.normalize()
}

const UMAX15: u32 = (1 << 15) - 1;
const UMAX2: u32 = (1 << 2) - 1;

#[inline]
pub fn encode_vec3_unorm_to_bits_15_15_2(x: f32, y: f32, z: f32) -> u32 {
    let x = (x.clamp(0.0, 1.0) * UMAX15 as f32).round() as u32;
    let y = (y.clamp(0.0, 1.0) * UMAX15 as f32).round() as u32;
    let z = (z.clamp(0.0, 1.0) * UMAX2 as f32).round() as u32;

    (x << 17) | (y << 2) | z
}

#[inline]
pub fn decode_bits_15_15_2_to_vec3(encoded: u32) -> (f32, f32, f32) {
    let x = ((encoded >> 17) & UMAX15) as f32 / UMAX15 as f32;
    let y = ((encoded >> 2) & UMAX15) as f32 / UMAX15 as f32;
    let z = (encoded & UMAX2) as f32 / UMAX2 as f32;

    (x, y, z)
}

const UMAX16: u32 = (1 << 16) - 1;

#[inline]
pub fn encode_vec2_unorm(v: &Vec2) -> u32 {
    let x = (v.x.clamp(0.0, 1.0) * UMAX16 as f32).round() as u32;
    let y = (v.y.clamp(0.0, 1.0) * UMAX16 as f32).round() as u32;

    (x << 16) | y
}

#[inline]
pub fn decode_vec2_unorm(encoded: u32) -> Vec2 {
    let x = ((encoded >> 16) & UMAX16) as f32 / UMAX16 as f32;
    let y = (encoded & UMAX16) as f32 / UMAX16 as f32;

    vec2(x, y)
}

const UMAX8: u32 = (1 << 8) - 1;

#[inline]
pub fn encode_vec4_unorm(v: &Vec4) -> u32 {
    let x = (v.x.clamp(0.0, 1.0) * UMAX8 as f32).round() as u32;
    let y = (v.y.clamp(0.0, 1.0) * UMAX8 as f32).round() as u32;
    let z = (v.z.clamp(0.0, 1.0) * UMAX8 as f32).round() as u32;
    let w = (v.w.clamp(0.0, 1.0) * UMAX8 as f32).round() as u32;

    (x << 24) | (y << 16) | (z << 8) | w
}

#[inline]
pub fn decode_vec4_unorm(encoded: u32) -> Vec4 {
    let x = ((encoded >> 24) & 0xFF) as f32 / UMAX8 as f32;
    let y = ((encoded >> 16) & 0xFF) as f32 / UMAX8 as f32;
    let z = ((encoded >> 8) & 0xFF) as f32 / UMAX8 as f32;
    let w = (encoded & 0xFF) as f32 / UMAX8 as f32;

    Vec4::new(x, y, z, w)
}

#[inline]
pub fn u16x4_to_u32(arr: &[u16; 4]) -> u32 {
    let byte1 = (arr[0] & 0xFF) as u32;
    let byte2 = (arr[1] & 0xFF) as u32;
    let byte3 = (arr[2] & 0xFF) as u32;
    let byte4 = (arr[3] & 0xFF) as u32;

    (byte1 << 24) | (byte2 << 16) | (byte3 << 8) | byte4
}

attribute vec4 Vertex_Tangent;
attribute vec3 Vertex_Position;
attribute vec3 Vertex_Normal;
attribute vec2 Vertex_Uv;
// attribute vec2 Vertex_Uv_1;
attribute vec4 Vertex_JointWeight;
attribute vec4 Vertex_JointIndex;

uniform mat4 world_from_local;
uniform mat4 joint_data[MAX_JOINTS];
uniform bool has_joint_data;

varying vec4 clip_position;
varying vec3 ws_position;
varying vec4 tangent;
varying vec3 vert_normal;
varying vec2 uv_0;

void main() {
    mat4 world_from_local = world_from_local;

    if (has_joint_data) {
        ivec4 indices = ivec4(Vertex_JointIndex);
        world_from_local = Vertex_JointWeight.x * joint_data[indices.x] +
                Vertex_JointWeight.y * joint_data[indices.y] +
                Vertex_JointWeight.z * joint_data[indices.z] +
                Vertex_JointWeight.w * joint_data[indices.w];
    }

    clip_position = (clip_from_world * world_from_local) * vec4(Vertex_Position, 1.0);
    gl_Position = clip_position;
    vert_normal = (world_from_local * vec4(Vertex_Normal, 0.0)).xyz;
    ws_position = (world_from_local * vec4(Vertex_Position, 1.0)).xyz;
    uv_0 = Vertex_Uv;
    tangent = Vertex_Tangent;
}

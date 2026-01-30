attribute vec4 Vertex_Tangent;
attribute vec3 Vertex_Position;
attribute vec3 Vertex_Normal;
attribute vec2 Vertex_Uv;
attribute vec2 Vertex_Uv_1;

uniform mat4 world_from_local;

varying vec4 clip_position;
varying vec3 ws_position;
varying vec4 tangent;
varying vec3 vert_normal;
varying vec2 uv_0;
varying vec2 uv_1;

void main() {
    mat4 world_from_local = world_from_local;

    clip_position = (ub_clip_from_world * world_from_local) * vec4(Vertex_Position, 1.0);
    gl_Position = clip_position;
    vert_normal = (world_from_local * vec4(Vertex_Normal, 0.0)).xyz;
    ws_position = (world_from_local * vec4(Vertex_Position, 1.0)).xyz;
    uv_0 = Vertex_Uv;
    uv_1 = Vertex_Uv_1;
    tangent = Vertex_Tangent;
}

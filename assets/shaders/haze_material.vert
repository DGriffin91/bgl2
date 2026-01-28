attribute vec4 Vertex_Tangent;
attribute vec3 Vertex_Position;
attribute vec3 Vertex_Normal;
attribute vec2 Vertex_Uv;

uniform mat4 ub_world_from_local;

varying vec4 clip_position;
varying vec3 ws_position;
varying vec4 tangent;
varying vec3 vert_normal;
varying vec2 uv_0;

void main() {
    clip_position = (ub_clip_from_world * ub_world_from_local) * vec4(Vertex_Position, 1.0);
    gl_Position = clip_position;
    vert_normal = (ub_world_from_local * vec4(Vertex_Normal, 0.0)).xyz;
    ws_position = (ub_world_from_local * vec4(Vertex_Position, 1.0)).xyz;
    uv_0 = Vertex_Uv;
    tangent = Vertex_Tangent;
}

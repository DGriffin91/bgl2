attribute vec4 Vertex_Tangent;
attribute vec3 Vertex_Position;
attribute vec3 Vertex_Normal;
attribute vec2 Vertex_Uv;
attribute vec2 Vertex_Uv_1;

uniform mat4 local_to_clip;
uniform mat4 local_to_world;
uniform mat4 view_to_world;

varying vec3 ws_position;
varying vec4 tangent;
varying vec3 normal;
varying vec2 uv_0;
varying vec2 uv_1;

void main() {
    gl_Position = local_to_clip * vec4(Vertex_Position, 1.0);
    normal = (local_to_world * vec4(Vertex_Normal, 0.0)).xyz;
    ws_position = (local_to_world * vec4(Vertex_Position, 1.0)).xyz;
    uv_0 = Vertex_Uv;
    uv_1 = Vertex_Uv_1;
    tangent = Vertex_Tangent;
}

attribute vec3 Vertex_Position;
attribute vec2 Vertex_Uv;

uniform mat4 ub_world_from_local;

varying vec2 uv_0;

void main() {
    gl_Position = (ub_clip_from_world * ub_world_from_local) * vec4(Vertex_Position, 1.0);
    uv_0 = Vertex_Uv;
}

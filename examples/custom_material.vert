attribute vec3 Vertex_Position;

uniform mat4 clip_from_local;

void main() {
    gl_Position = clip_from_local * vec4(Vertex_Position, 1.0);
}

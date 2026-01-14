attribute vec3 Vertex_Position;

uniform mat4 clip_from_local;

varying vec4 clip_position;

void main() {
    clip_position = clip_from_local * vec4(Vertex_Position, 1.0);
    gl_Position = clip_position;
}

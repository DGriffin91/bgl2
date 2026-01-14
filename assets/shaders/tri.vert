attribute vec2 a_position;
varying vec2 vert;

void main() {
vert = a_position;
    gl_Position = vec4(a_position - vec2(0.5, 0.5), 0.0, 1.0);
}

uniform vec4 color;
//uniform sampler2D emissive;
varying vec4 clip_position;

void main() {
    gl_FragColor = color; // * texture2D(emissive, clip_position.xy);
}

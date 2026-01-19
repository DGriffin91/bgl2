varying vec4 clip_position;
varying vec3 ws_position;
varying vec4 tangent;
varying vec3 vert_normal;
varying vec2 uv_0;
varying vec2 uv_1;


uniform vec3 view_position;
uniform vec2 view_resolution;
uniform float view_exposure;

uniform vec4 base_color;
uniform vec4 emissive;
uniform vec3 reflectance;
uniform float metallic;
uniform float perceptual_roughness;
uniform float diffuse_transmission;

uniform bool double_sided;
uniform bool flip_normal_map_y;
uniform bool alpha_blend;

uniform sampler2D base_color_texture;
uniform sampler2D normal_map_texture;
uniform bool has_normal_map;
uniform sampler2D metallic_roughness_texture;
uniform sampler2D emissive_texture;

uniform sampler2D reflect_texture;
uniform bool read_reflection;
uniform bool write_reflection;
uniform vec3 reflection_plane_position;
uniform vec3 reflection_plane_normal;






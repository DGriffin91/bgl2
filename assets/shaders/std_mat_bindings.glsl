#define MAX_POINT_LIGHTS 32

varying vec4 clip_position;
varying vec3 ws_position;
varying vec4 tangent;
varying vec3 vert_normal;
varying vec2 uv_0;
varying vec2 uv_1;

uniform mat4 shadow_clip_from_world;
uniform vec3 directional_light_dir_to_light;
uniform vec3 directional_light_color;

uniform vec3 view_position;
uniform vec2 view_resolution;

uniform vec4 base_color;
uniform float metallic;
uniform float perceptual_roughness;

uniform bool double_sided;
uniform bool flip_normal_map_y;
uniform bool alpha_blend;
uniform int flags;

uniform sampler2D base_color_texture;
uniform sampler2D normal_map_texture;
uniform bool has_normal_map;
uniform sampler2D metallic_roughness_texture;
uniform samplerCube specular_map;
uniform samplerCube diffuse_map;
uniform float env_intensity;

uniform sampler2D shadow_texture;
uniform sampler2D reflect_texture;
uniform bool read_reflection;
uniform bool write_reflection;
uniform vec3 reflection_plane_position;
uniform vec3 reflection_plane_normal;

uniform int light_count;
uniform vec4 point_light_position_range[MAX_POINT_LIGHTS];
uniform vec4 point_light_color_radius[MAX_POINT_LIGHTS];
uniform vec4 spot_light_dir_offset_scale[MAX_POINT_LIGHTS];

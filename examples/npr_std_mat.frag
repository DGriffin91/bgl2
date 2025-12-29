
//#include agx

varying vec4 clip_position;
varying vec3 ws_position;
varying vec4 tangent;
varying vec3 vert_normal;
varying vec2 uv_0;
varying vec2 uv_1;

uniform mat4 shadow_clip_from_world;
uniform vec3 directional_light_dir_to_light;

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
uniform sampler2D metallic_roughness_texture;
uniform samplerCube specular_map;
uniform samplerCube diffuse_map;

uniform sampler2D shadow_texture;

// https://aras-p.info/blog/2009/07/30/encoding-floats-to-rgba-the-final/
vec4 EncodeFloatRGBA(float v) {
    vec4 enc = vec4(1.0, 255.0, 65025.0, 16581375.0) * clamp(v, 0.0, 1.0);
    enc = fract(enc);
    enc -= enc.yzww * vec4(1.0 / 255.0, 1.0 / 255.0, 1.0 / 255.0, 0.0);
    return enc;
}
float DecodeFloatRGBA(vec4 rgba) {
    return clamp(dot(rgba, vec4(1.0, 1.0 / 255.0, 1.0 / 65025.0, 1.0 / 16581375.0)), 0.0, 1.0);
}

vec3 rgbe2rgb(vec4 rgbe) {
    return (rgbe.rgb * pow(2.0, rgbe.a * 255.0 - 128.0));
}

#include shadow_sampling

// http://www.mikktspace.com/
vec3 apply_normal_mapping(vec3 ws_normal, vec4 ws_tangent, vec2 uv) {
    vec3 N = ws_normal;
    vec3 T = ws_tangent.xyz;
    vec3 B = ws_tangent.w * cross(N, T);
    vec3 Nt = texture2D(normal_map_texture, uv).rgb * 2.0 - 1.0; // Only supports 3-component normal maps
    if (flip_normal_map_y) {
        Nt.y = -Nt.y;
    }
    if (double_sided && !gl_FrontFacing) {
        Nt = -Nt;
    }
    N = Nt.x * T + Nt.y * B + Nt.z * N;
    return normalize(N);
}

void main() {
    vec4 color = base_color * texture2D(base_color_texture, uv_0);

    if (!alpha_blend && (color.a < 0.5)) {
        discard;
    }

    vec3 ndc_position = clip_position.xyz / clip_position.w;

    #ifdef RENDER_SHADOW
    gl_FragColor = EncodeFloatRGBA(clamp(ndc_position.z * 0.5 + 0.5, 0.0, 1.0));
    #else

    float shadow = 1.0;
    #ifdef SAMPLE_SHADOW
    float bias = 0.002;
    float normal_bias = 0.05;

    vec4 shadow_clip = shadow_clip_from_world * vec4(ws_position + vert_normal * normal_bias, 1.0);
    vec3 shadow_ndc = shadow_clip.xyz / shadow_clip.w;
    float receiver_z = shadow_ndc.z * 0.5 + 0.5;
    vec2 shadow_uv = shadow_ndc.xy * 0.5 + 0.5;
    vec2 step = 1.0 / view_resolution;
    float sum = 0.0;

    if (shadow_uv.x > 0.0 && shadow_uv.x < 1.0 && shadow_uv.y > 0.0 && shadow_uv.y < 1.0) {
        shadow *= bilinear_shadow2(shadow_uv, receiver_z, bias, view_resolution);
        //shadow *= sample_shadow_map_castano_thirteen(shadow_uv, receiver_z, bias, view_resolution);
    }
    #endif // SAMPLE_SHADOW

    vec3 light_dir = vec3(0.0, 1.0, 0.0);
    vec3 light_color = vec3(0.0);
    if (directional_light_dir_to_light != vec3(0.0)) {
        light_dir = normalize(directional_light_dir_to_light);
        light_color = vec3(1.0, 0.9, 0.8) * 3.0;
    }

    float specular_intensity = 1.0;

    vec3 V = normalize(ws_position - view_position);
    vec3 view_dir = normalize(view_position - ws_position);

    vec4 metallic_roughness = texture2D(metallic_roughness_texture, uv_0);
    float perceptual_roughness_tex = metallic_roughness.g * perceptual_roughness; // TODO better name
    float roughness = perceptual_roughness_tex * perceptual_roughness_tex;

    vec3 normal = apply_normal_mapping(vert_normal, tangent, uv_0);

    // https://en.wikipedia.org/wiki/Blinn%E2%80%93Phong_reflection_model
    float lambert = dot(light_dir, normal);

    vec3 half_dir = normalize(light_dir + view_dir);
    float spec_angle = max(dot(half_dir, normal), 0.0);
    float shininess = mix(0.0, 64.0, (1.0 - roughness));
    float specular = pow(spec_angle, shininess);
    specular = specular * pow(min(lambert + 1.0, 1.0), 4.0); // Fade out spec TODO improve

    float metallic = metallic * metallic_roughness.b;
    vec3 diffuse_color = shadow * color.rgb * lambert * light_color * (1.0 - metallic);
    vec3 specular_color = shadow * specular * light_color * specular_intensity;
    specular_color = mix(specular_color, specular_color * color.rgb, vec3(metallic));

    float mip_levels = 8.0; // TODO put in uniform
    vec4 specular_env_color = textureCubeLod(specular_map, reflect(V, normal), perceptual_roughness_tex * mip_levels);
    vec4 diffuse_env_color = textureCubeLod(diffuse_map, normal, 0.0);
    #ifdef WEBGL1
    specular_env_color.rgb = rgbe2rgb(specular_env_color);
    diffuse_env_color.rgb = rgbe2rgb(diffuse_env_color);
    #endif
    specular_color += specular_env_color.rgb * specular_intensity;
    diffuse_color += color.rgb * diffuse_env_color.rgb * (1.0 - metallic);

    lambert = max(lambert, 0.0);
    gl_FragColor = vec4(diffuse_color + specular_color, color.a);
    //gl_FragColor.rgb = pow(agx_tonemapping(gl_FragColor.rgb), vec3(2.2)); //Convert back to linear
    gl_FragColor.rgb = gl_FragColor.rgb * 0.4;
    gl_FragColor = clamp(gl_FragColor, vec4(0.0), vec4(1.0));

    #endif // NOT RENDER_SHADOW
}

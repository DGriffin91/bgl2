#include agx

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

uniform sampler2D shadow_texture;

// https://aras-p.info/blog/2009/07/30/encoding-floats-to-rgba-the-final/
vec4 EncodeFloatRGBA(float v) {
    vec4 enc = vec4(1.0, 255.0, 65025.0, 16581375.0) * clamp(v, 0.0, 1.0);
    enc = fract(enc);
    enc -= enc.yzww * vec4(1.0 / 255.0, 1.0 / 255.0, 1.0 / 255.0, 0.0);
    return enc;
}
float DecodeFloatRGBA(vec4 rgba) {
    return clamp(dot(rgba, vec4(1.0, 1 / 255.0, 1 / 65025.0, 1 / 16581375.0)), 0.0, 1.0);
}

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

float bilinear_shadow(vec2 uv, float receiver_z, float bias, vec2 shadow_res) {
    vec2 step = 1.0 / shadow_res;

    vec2 p = (uv * shadow_res - 0.5);
    vec2 pos = floor(p) * step;
    vec2 f = fract(p);

    float t00 = float(receiver_z > DecodeFloatRGBA(texture2D(shadow_texture, pos + vec2(0.0, 0.0))) - bias);
    float t10 = float(receiver_z > DecodeFloatRGBA(texture2D(shadow_texture, pos + vec2(step.x, 0.0))) - bias);
    float t01 = float(receiver_z > DecodeFloatRGBA(texture2D(shadow_texture, pos + vec2(0.0, step.y))) - bias);
    float t11 = float(receiver_z > DecodeFloatRGBA(texture2D(shadow_texture, pos + vec2(step.x, step.y))) - bias);

    return mix(mix(t00, t10, f.x), mix(t01, t11, f.x), f.y);
}

float bilinear_shadow2(vec2 uv, float receiver_z, float bias, vec2 shadow_res) {
    vec2 step = 1.0 / shadow_res;

    vec2 p = (uv * shadow_res - 0.5);
    vec2 pos = floor(p) * step;
    vec2 f = fract(p);

    float t00 = bilinear_shadow(pos + vec2(0.0, 0.0), receiver_z, bias, shadow_res);
    float t10 = bilinear_shadow(pos + vec2(step.x, 0.0), receiver_z, bias, shadow_res);
    float t01 = bilinear_shadow(pos + vec2(0.0, step.y), receiver_z, bias, shadow_res);
    float t11 = bilinear_shadow(pos + vec2(step.x, step.y), receiver_z, bias, shadow_res);

    return mix(mix(t00, t10, f.x), mix(t01, t11, f.x), f.y);
}

float bilinear_shadow_cont(vec2 uv, float receiver_z, float bias, vec2 shadow_res) {
    vec2 step = 1.0 / shadow_res;

    vec2 p = (uv * shadow_res - 0.5);
    vec2 pos = floor(p) * step;
    vec2 f = fract(p);

    float t00 = DecodeFloatRGBA(texture2D(shadow_texture, pos + vec2(0.0, 0.0)));
    float t10 = DecodeFloatRGBA(texture2D(shadow_texture, pos + vec2(step.x, 0.0)));
    float t01 = DecodeFloatRGBA(texture2D(shadow_texture, pos + vec2(0.0, step.y)));
    float t11 = DecodeFloatRGBA(texture2D(shadow_texture, pos + vec2(step.x, step.y)));

    float result = mix(mix(t00, t10, f.x), mix(t01, t11, f.x), f.y);

    return float(receiver_z > result - bias);
}

float sample_shadow_map_castano_thirteen(vec2 uv, float receiver_z, float bias, vec2 shadow_res) {
    vec2 inv_map_size = vec2(1.0) / shadow_res;

    uv = uv * shadow_res;
    vec2 base_uv = floor(uv + vec2(0.5));
    float s = (uv.x + 0.5 - base_uv.x);
    float t = (uv.y + 0.5 - base_uv.y);
    base_uv -= 0.5;
    base_uv *= inv_map_size;

    float uw0 = (4.0 - 3.0 * s);
    float uw1 = 7.0;
    float uw2 = (1.0 + 3.0 * s);

    float u0 = (3.0 - 2.0 * s) / uw0 - 2.0;
    float u1 = (3.0 + s) / uw1;
    float u2 = s / uw2 + 2.0;

    float vw0 = (4.0 - 3.0 * t);
    float vw1 = 7.0;
    float vw2 = (1.0 + 3.0 * t);

    float v0 = (3.0 - 2.0 * t) / vw0 - 2.0;
    float v1 = (3.0 + t) / vw1;
    float v2 = t / vw2 + 2.0;

    float sum = 0.0;

    sum += uw0 * vw0 * bilinear_shadow(base_uv + vec2(u0, v0) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw1 * vw0 * bilinear_shadow(base_uv + vec2(u1, v0) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw2 * vw0 * bilinear_shadow(base_uv + vec2(u2, v0) * inv_map_size, receiver_z, bias, shadow_res);

    sum += uw0 * vw1 * bilinear_shadow(base_uv + vec2(u0, v1) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw1 * vw1 * bilinear_shadow(base_uv + vec2(u1, v1) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw2 * vw1 * bilinear_shadow(base_uv + vec2(u2, v1) * inv_map_size, receiver_z, bias, shadow_res);

    sum += uw0 * vw2 * bilinear_shadow(base_uv + vec2(u0, v2) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw1 * vw2 * bilinear_shadow(base_uv + vec2(u1, v2) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw2 * vw2 * bilinear_shadow(base_uv + vec2(u2, v2) * inv_map_size, receiver_z, bias, shadow_res);

    return sum / 144.0;
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
    float roughness = metallic_roughness.g * perceptual_roughness;
    roughness *= roughness;

    vec3 normal = apply_normal_mapping(vert_normal, tangent, uv_0);

    // https://en.wikipedia.org/wiki/Blinn%E2%80%93Phong_reflection_model
    float lambert = dot(light_dir, normal);

    vec3 half_dir = normalize(light_dir + view_dir);
    float spec_angle = max(dot(half_dir, normal), 0.0);
    float shininess = mix(0.0, 64.0, (1.0 - roughness));
    float specular = pow(spec_angle, shininess);
    specular = specular * pow(min(lambert + 1.0, 1.0), 4.0); // Fade out spec TODO improve

    float metallic = metallic * metallic_roughness.b;
    vec3 diffuse_color = color.rgb * lambert * light_color * (1.0 - metallic);
    vec3 specular_color = specular * light_color * specular_intensity;
    specular_color = mix(specular_color, specular_color * color.rgb, vec3(metallic));

    lambert = max(lambert, 0.0);
    gl_FragColor = vec4(diffuse_color + specular_color, color.a);
    gl_FragColor.rgb = pow(agx_tonemapping(gl_FragColor.rgb), vec3(2.2)); //Convert back to linear
    gl_FragColor = clamp(gl_FragColor, vec4(0.0), vec4(1.0));

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
        gl_FragColor.xyz *= bilinear_shadow2(shadow_uv, receiver_z, bias, view_resolution);
        //gl_FragColor.xyz *= sample_shadow_map_castano_thirteen(shadow_uv, receiver_z, bias, view_resolution);
    }
    #endif // SAMPLE_SHADOW

    #endif // NOT RENDER_SHADOW
}

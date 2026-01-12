
//#include agx

#define MAX_POINT_LIGHTS 32
#define POINT_LIGHT_PRE_EXPOSE 0.0001

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
uniform bool has_normal_map;
uniform sampler2D metallic_roughness_texture;
uniform samplerCube specular_map;
uniform samplerCube diffuse_map;

uniform sampler2D shadow_texture;
uniform sampler2D reflect_texture;
uniform bool read_reflection;

uniform int light_count;
uniform vec4 point_light_position_range[MAX_POINT_LIGHTS];
uniform vec4 point_light_color_radius[MAX_POINT_LIGHTS];
uniform vec4 spot_light_dir_offset_scale[MAX_POINT_LIGHTS];

// For decoding normals or unit direction vectors from octahedral coordinates.
vec3 octahedral_decode(vec2 v) {
    vec2 f = v * 2.0 - 1.0;
    vec3 n = vec3(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));
    float t = clamp(-n.z, 0.0, 1.0);
    vec2 w = vec2(t);
    if (n.x > 0.0) {
        w.x = -w.x;
    }
    if (n.y > 0.0) {
        w.y = -w.y;
    }
    return normalize(vec3(n.x + w.x, n.y + w.y, n.z));
}

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

float get_distance_attenuation(float distance, float range) {
    float distanceSquare = distance * distance;
    float inverseRangeSquared = 1.0 / (range * range);
    float factor = distanceSquare * inverseRangeSquared;
    float smoothFactor = clamp(1.0 - factor * factor, 0.0, 1.0);
    float attenuation = smoothFactor * smoothFactor;
    return max(attenuation * 1.0 / max(distanceSquare, 0.0001), 0.0);
}

void main() {
    vec4 color = base_color * texture2D(base_color_texture, uv_0);


    if (!alpha_blend && (color.a < 0.5)) {
        discard;
    }

    vec3 ndc_position = clip_position.xyz / clip_position.w;
    vec2 screen_uv = ndc_position.xy * 0.5 + 0.5;

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

    vec3 normal = vert_normal;
    if (has_normal_map) {
        normal = apply_normal_mapping(vert_normal, tangent, uv_0);
    }

    vec3 specular_color;
    vec3 diffuse_color;

    float shininess = mix(0.0, 64.0, (1.0 - roughness));
    {
        // Directional Light
        // https://en.wikipedia.org/wiki/Blinn%E2%80%93Phong_reflection_model
        float lambert = max(dot(light_dir, normal), 0.0);

        vec3 half_dir = normalize(light_dir + view_dir);
        float spec_angle = max(dot(half_dir, normal), 0.0);
        float specular = pow(spec_angle, shininess);
        specular = specular * pow(min(lambert + 1.0, 1.0), 4.0); // Fade out spec TODO improve

        float metallic = metallic * metallic_roughness.b;
        diffuse_color += shadow * color.rgb * lambert * light_color * (1.0 - metallic);
        vec3 dir_light_specular = shadow * specular * light_color * specular_intensity;
        specular_color += mix(dir_light_specular, dir_light_specular * color.rgb, vec3(metallic));
    }



    {
        // Environment map / reflection
        float mip_levels = 8.0; // TODO put in uniform

        vec4 diffuse_env_color = textureCubeLod(diffuse_map, normal, 0.0);
        #ifdef WEBGL1
        diffuse_env_color.rgb = rgbe2rgb(diffuse_env_color);
        #endif
        diffuse_color += color.rgb * diffuse_env_color.rgb * (1.0 - metallic);

        vec3 env_specular;
        if (read_reflection && perceptual_roughness < 0.2) {
            vec3 sharp_reflection_color = texture2D(reflect_texture, screen_uv).rgb;
            env_specular = sharp_reflection_color.rgb * specular_intensity;
        } else {
            vec4 specular_env_color = textureCubeLod(specular_map, reflect(V, normal), perceptual_roughness_tex * mip_levels);
            #ifdef WEBGL1
            specular_env_color.rgb = rgbe2rgb(specular_env_color);
            #endif
            env_specular = specular_env_color.rgb * specular_intensity;
        }
        specular_color += mix(env_specular, env_specular * color.rgb, vec3(metallic));
    }



    // Point Lights
#ifdef WEBGL1
    for (int i = 0; i < MAX_POINT_LIGHTS; i++) {
#else
    for (int i = 0; i < light_count; i++) {
#endif //WEBGL1
        vec4 light_position_range = point_light_position_range[i];
        vec3 to_light = light_position_range.xyz - ws_position;
        float distance = length(to_light);
        bool in_range = distance < light_position_range.w;
#ifdef WEBGL1
        in_range = in_range && (i < MAX_POINT_LIGHTS);
#endif
        if (!in_range) {
            continue;
        }

        vec4 light_color_radius = point_light_color_radius[i];
        vec3 light_color = light_color_radius.rgb * POINT_LIGHT_PRE_EXPOSE;

        vec3 to_light_dir = normalize(to_light);
        vec3 half_dir = normalize(to_light_dir + view_dir);

        float dist_attenuation = get_distance_attenuation(distance, light_position_range.w);
        float lambert = max(dot(to_light_dir, normal), 0.0);

        float spot_attenuation = 1.0;
        {
            // https://google.github.io/filament/Filament.html#listing_glslpunctuallight
            vec4 light_dir_offset_scale = spot_light_dir_offset_scale[i];
            vec3 spot_dir = octahedral_decode(light_dir_offset_scale.xy);
            float spot_offset = light_dir_offset_scale.z;
            float spot_scale = light_dir_offset_scale.w;
            float attenuation = clamp(dot(-spot_dir, to_light_dir) * spot_scale + spot_offset, 0.0, 1.0);
            spot_attenuation = attenuation * attenuation;
        }

        diffuse_color += color.rgb * (1.0 - metallic) * light_color * lambert * dist_attenuation * spot_attenuation;

        float spec_angle = max(dot(half_dir, normal), 0.0);
        vec3 specular = pow(spec_angle, shininess) * light_color * specular_intensity;
        specular_color += mix(specular, specular * color.rgb, vec3(metallic)) * dist_attenuation * spot_attenuation;
    }

    gl_FragColor = vec4(diffuse_color + specular_color, color.a);
    //gl_FragColor.rgb = pow(agx_tonemapping(gl_FragColor.rgb), vec3(2.2)); //Convert back to linear
    gl_FragColor.rgb = gl_FragColor.rgb * 0.4;
    gl_FragColor = clamp(gl_FragColor, vec4(0.0), vec4(1.0));

    #endif // NOT RENDER_SHADOW
}

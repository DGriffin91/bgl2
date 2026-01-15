#define POINT_LIGHT_PRE_EXPOSE 0.00005
#define ENV_LIGHT_PRE_EXPOSE 0.001
#define DIR_LIGHT_PRE_EXPOSE 0.0001

//#include agx
#include std_mat_bindings
#include math
#include shadow_sampling

// http://www.mikktspace.com/
vec3 apply_normal_mapping(sampler2D normal_tex, vec3 ws_normal, vec4 ws_tangent, vec2 uv) {
    vec3 N = ws_normal;
    vec3 T = ws_tangent.xyz;
    vec3 B = ws_tangent.w * cross(N, T);
    vec3 Nt = texture2D(normal_tex, uv).rgb * 2.0 - 1.0; // Only supports 3-component normal maps
    if (flip_normal_map_y) {
        Nt.y = -Nt.y;
    }
    if (double_sided && !gl_FrontFacing) {
        Nt = -Nt;
    }
    N = Nt.x * T + Nt.y * B + Nt.z * N;
    return normalize(N);
}

float distance_attenuation(float distance, float range) {
    float distanceSquare = distance * distance;
    float inverseRangeSquared = 1.0 / (range * range);
    float factor = distanceSquare * inverseRangeSquared;
    float smoothFactor = clamp(1.0 - factor * factor, 0.0, 1.0);
    float attenuation = smoothFactor * smoothFactor;
    return max(attenuation * 1.0 / max(distanceSquare, 0.0001), 0.0);
}

float spot_angle_attenuation(vec3 spot_dir, vec3 to_light_dir, float spot_offset, float spot_scale) {
    float attenuation = clamp(dot(-spot_dir, to_light_dir) * spot_scale + spot_offset, 0.0, 1.0);
    return attenuation * attenuation;
}

void main() {
    vec4 color = base_color * texture2D(base_color_texture, uv_0);

    if (!alpha_blend && (color.a < 0.5)) {
        discard;
    }
    if (write_reflection) {
        if (dot(ws_position - reflection_plane_position, reflection_plane_normal) < 0.0) {
            discard;
        }
    }

    vec3 ndc_position = clip_position.xyz / clip_position.w;
    vec2 screen_uv = ndc_position.xy * 0.5 + 0.5;

    #ifdef RENDER_SHADOW
    gl_FragColor = EncodeFloatRGBA(clamp(ndc_position.z * 0.5 + 0.5, 0.0, 1.0));
    #else // RENDER_SHADOW

    float dir_shadow = 1.0;
    #ifdef SAMPLE_SHADOW
    float bias = 0.002;
    float normal_bias = 0.05;

    vec4 shadow_clip = shadow_clip_from_world * vec4(ws_position + vert_normal * normal_bias, 1.0);
    vec3 shadow_ndc = shadow_clip.xyz / shadow_clip.w;
    float receiver_z = shadow_ndc.z * 0.5 + 0.5;
    vec2 shadow_uv = shadow_ndc.xy * 0.5 + 0.5;

    if (shadow_uv.x > 0.0 && shadow_uv.x < 1.0 && shadow_uv.y > 0.0 && shadow_uv.y < 1.0) {
        dir_shadow *= bilinear_shadow2(shadow_texture, shadow_uv, receiver_z, bias, view_resolution);
        //dir_shadow *= sample_shadow_map_castano_thirteen(shadow_texture, shadow_uv, receiver_z, bias, view_resolution);
    }
    #endif // SAMPLE_SHADOW

    float specular_intensity = 1.0;

    vec3 V = normalize(view_position - ws_position);

    vec4 metallic_roughness = texture2D(metallic_roughness_texture, uv_0);
    float perceptual_roughness = metallic_roughness.g * perceptual_roughness; // TODO better name
    float roughness = perceptual_roughness * perceptual_roughness;
    float metallic = metallic * metallic_roughness.b;

    vec3 emissive = emissive.rgb * texture2D(emissive_texture, uv_0).rgb;

    vec3 normal = vert_normal;
    if (has_normal_map) {
        normal = apply_normal_mapping(normal_map_texture, vert_normal, tangent, uv_0);
    }

    vec3 specular_color = vec3(0.0);
    vec3 diffuse_color = vec3(0.0);

    float shininess = mix(0.0, 64.0, (1.0 - roughness));
    if (dir_shadow > 0.0 && directional_light_dir_to_light != vec3(0.0)) {
        vec3 light_color = directional_light_color * DIR_LIGHT_PRE_EXPOSE;

        // Directional Light
        // https://en.wikipedia.org/wiki/Blinn%E2%80%93Phong_reflection_model
        float lambert = max(dot(directional_light_dir_to_light, normal), 0.0);

        vec3 half_dir = normalize(directional_light_dir_to_light + V);
        float spec_angle = max(dot(half_dir, normal), 0.0);
        float specular = pow(spec_angle, shininess);
        specular = specular * pow(min(lambert + 1.0, 1.0), 4.0); // Fade out spec TODO improve

        diffuse_color += dir_shadow * color.rgb * lambert * light_color * (1.0 - metallic);
        vec3 dir_light_specular = dir_shadow * specular * light_color * specular_intensity;
        specular_color += mix(dir_light_specular, dir_light_specular * color.rgb, vec3(metallic));
    }

    {
        // Environment map / reflection
        float mip_levels = 8.0; // TODO put in uniform

        vec4 diffuse_env_color = textureCubeLod(diffuse_map, normal, 0.0);
        #ifdef WEBGL1
        diffuse_env_color.rgb = rgbe2rgb(diffuse_env_color);
        #endif
        diffuse_color += color.rgb * diffuse_env_color.rgb * (1.0 - metallic) * env_intensity * ENV_LIGHT_PRE_EXPOSE;

        vec3 env_specular;
        if (read_reflection && perceptual_roughness < 0.2) {
            vec3 sharp_reflection_color = texture2D(reflect_texture, screen_uv).rgb;
            env_specular = sharp_reflection_color.rgb * specular_intensity;
        } else {
            vec4 specular_env_color = textureCubeLod(specular_map, reflect(-V, normal), perceptual_roughness * mip_levels);
            #ifdef WEBGL1
            specular_env_color.rgb = rgbe2rgb(specular_env_color);
            #endif
            env_specular = specular_env_color.rgb * specular_intensity * env_intensity * ENV_LIGHT_PRE_EXPOSE;
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
            vec3 half_dir = normalize(to_light_dir + V);

            float dist_attenuation = distance_attenuation(distance, light_position_range.w);
            float lambert = max(dot(to_light_dir, normal), 0.0);

            vec4 dos = spot_light_dir_offset_scale[i];
            float spot_attenuation = spot_angle_attenuation(octahedral_decode(dos.xy), to_light_dir, dos.z, dos.w);

            diffuse_color += color.rgb * (1.0 - metallic) * light_color * lambert * dist_attenuation * spot_attenuation;

            float spec_angle = max(dot(half_dir, normal), 0.0);
            vec3 specular = pow(spec_angle, shininess) * light_color * specular_intensity;
            specular_color += mix(specular, specular * color.rgb, vec3(metallic)) * dist_attenuation * spot_attenuation;
        }

        gl_FragColor = vec4(diffuse_color + specular_color + emissive, color.a);
        gl_FragColor = clamp(gl_FragColor, vec4(0.0), vec4(1.0));

        #endif // NOT RENDER_SHADOW
    }

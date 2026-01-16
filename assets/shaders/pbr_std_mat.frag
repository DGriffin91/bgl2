#include agx
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
    float att = smoothFactor * smoothFactor;
    return max(att * 1.0 / max(distanceSquare, 0.0001), 0.0);
}

float spot_angle_attenuation(vec3 spot_dir, vec3 to_light_dir, float spot_offset, float spot_scale) {
    float attenuation = clamp(dot(-spot_dir, to_light_dir) * spot_scale + spot_offset, 0.0, 1.0);
    return attenuation * attenuation;
}

// https://google.github.io/filament/Filament.html#materialsystem/parameterization/remapping
vec3 calculate_F0(vec3 base_color, float metallic) {
    float reflectance = 0.5;
    return 0.16 * reflectance * reflectance * (1.0 - metallic) + base_color * metallic;
}

float D_GGX(float NoH, float a) {
    float a2 = a * a;
    float f = (NoH * a2 - NoH) * NoH + 1.0;
    return a2 / (PI * f * f);
}

vec3 F_Schlick(float u, vec3 F0) {
    return F0 + (vec3(1.0) - F0) * pow(1.0 - u, 5.0);
}

float V_SmithGGXCorrelated(float NoV, float NoL, float a) {
    float a2 = a * a;
    float GGXL = NoV * sqrt((-NoL * a2 + NoL) * NoL + a2);
    float GGXV = NoL * sqrt((-NoV * a2 + NoV) * NoV + a2);
    return 0.5 / (GGXV + GGXL);
}

float Fd_Lambert() {
    return 1.0 / PI;
}

vec3 specular_brdf(vec3 V, vec3 L, vec3 normal, float roughness, vec3 F0) {
    vec3 H = normalize(V + L);
    float NoL = clamp(dot(normal, L), 0.0, 1.0);
    float NoV = abs(dot(normal, V)) + 1e-5;
    float NoH = clamp(dot(normal, H), 0.0, 1.0);
    float LoH = clamp(dot(L, H), 0.0, 1.0);

    float D = D_GGX(NoH, roughness);
    vec3 F = F_Schlick(LoH, F0);
    float VGGX = V_SmithGGXCorrelated(NoV, NoL, roughness);

    // specular BRDF
    vec3 Fr = clamp((D * VGGX) * F, vec3(0.0), vec3(1.0));

    return Fr;
}

// https://www.unrealengine.com/en-US/blog/physically-based-shading-on-mobile
vec2 F_AB(float Roughness, float NoV) {
    vec4 c0 = vec4(-1.0, -0.0275, -0.572, 0.022);
    vec4 c1 = vec4(1.0, 0.0425, 1.04, -0.04);
    vec4 r = Roughness * c0 + c1;
    float a004 = min(r.x * r.x, exp2(-9.28 * NoV)) * r.x + r.y;
    return vec2(-1.04, 1.04) * a004 + r.zw;
}


vec3 rgbe2rgb(vec4 rgbe) {
    return (rgbe.rgb * exp2(rgbe.a * 255.0 - 128.0) * 0.99609375); // (255.0/256.0)
}

void main() {
    vec4 color = base_color * to_linear(texture2D(base_color_texture, uv_0));

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

    // -----------------------------

    float specular_intensity = 1.0;

    vec3 V = normalize(view_position - ws_position);

    vec4 metallic_roughness = texture2D(metallic_roughness_texture, uv_0);
    float perceptual_roughness = metallic_roughness.g * perceptual_roughness;
    float roughness = perceptual_roughness * perceptual_roughness;
    float metallic = metallic * metallic_roughness.b;
    vec3 F0 = calculate_F0(color.rgb, metallic);

    vec3 emissive = view_exposure * emissive.rgb * to_linear(texture2D(emissive_texture, uv_0).rgb);

    vec3 normal = vert_normal;
    if (has_normal_map) {
        normal = apply_normal_mapping(normal_map_texture, vert_normal, tangent, uv_0);
    }
    float NoV = abs(dot(normal, V)) + 1e-5;

    vec3 specular_color = vec3(0.0);
    vec3 diffuse_color = vec3(0.0);

    if (dir_shadow > 0.0 && directional_light_dir != vec3(0.0)) {
        // Directional Light
        vec3 light_color = directional_light_color;

        vec3 L = normalize(-directional_light_dir);
        vec3 half_dir = normalize(L + V);

        float NoL = clamp(dot(normal, L), 0.0, 1.0);
        diffuse_color += view_exposure * dir_shadow * color.rgb * (1.0 - metallic) * Fd_Lambert() * NoL * light_color;
        specular_color += view_exposure * dir_shadow * specular_brdf(V, L, normal, roughness, F0) * NoL * light_color;
    }

    {
        // Environment map / reflection
        float mip_levels = 8.0; // TODO put in uniform

        vec3 env_diffuse = rgbe2rgb(textureCubeLod(diffuse_map, vec3(normal.xy, -normal.z), 0.0)) * env_intensity;

        vec3 env_specular = vec3(0.0);
        if (read_reflection && perceptual_roughness < 0.2) {
            vec3 sharp_reflection_color = texture2D(reflect_texture, screen_uv).rgb;
            // TODO integrate properly (invert tonemapping? blend post tonemapping?)
            specular_color += sharp_reflection_color.rgb * F0 * 10.0; 
        } else {
            vec3 dir = reflect(-V, normal);
            env_specular = rgbe2rgb(textureCubeLod(specular_map, vec3(dir.xy, -dir.z), perceptual_roughness * mip_levels)) * env_intensity;
        }

        vec2 f_ab = F_AB(perceptual_roughness, NoV);


        // Multiscattering approximation
        // https://bruop.github.io/ibl
        // https://www.jcgt.org/published/0008/01/03/paper.pdf
        // vec3 Fr = max(vec3(1.0 - roughness), F0) - F0;
        // vec3 kS = F0 + Fr * pow(1.0 - NoV, 5.0);
        vec3 FssEss = F0 * f_ab.x + f_ab.y; // Optionally use kS in place of F0 here
        float Ems = (1.0 - (f_ab.x + f_ab.y));
        vec3 F_avg = F0 + (1.0 - F0) / 21.0;
        vec3 FmsEms = Ems * FssEss * F_avg / (1.0 - F_avg * Ems);
        vec3 k_D = color.rgb * (1.0 - FssEss - FmsEms);

        diffuse_color += view_exposure * (FmsEms + k_D) * env_diffuse;
        specular_color += view_exposure * FssEss * env_specular;
    }

    // Point Lights
    for (int i = 0; i < MAX_POINT_LIGHTS; i++) {
        if (i < light_count) {
            vec4 light_position_range = point_light_position_range[i];
            vec3 to_light = light_position_range.xyz - ws_position;
            float distance = length(to_light);
            if (distance < light_position_range.w) {
                vec4 light_color_radius = point_light_color_radius[i];
                vec3 light_color = light_color_radius.rgb;

                vec3 L = normalize(to_light);
                vec4 dos = spot_light_dir_offset_scale[i];
                float spot_attenuation = spot_angle_attenuation(octahedral_decode(dos.xy), L, dos.z, dos.w);
                float dist_attenuation = distance_attenuation(distance, light_position_range.w);

                float attenuation = dist_attenuation * spot_attenuation;
                float NoL = clamp(dot(normal, L), 0.0, 1.0);

                diffuse_color += view_exposure * color.rgb * (1.0 - metallic) * Fd_Lambert() * NoL * attenuation * light_color;
                specular_color += view_exposure * specular_brdf(V, L, normal, roughness, F0) * NoL * attenuation * light_color;
            }
        }
    }

    gl_FragColor = vec4(diffuse_color + specular_color + emissive.rgb, color.a);
    //gl_FragColor.rgb = agx_tonemapping(gl_FragColor.rgb); // in: linear, out: srgb
    gl_FragColor.rgb = from_linear(gl_FragColor.rgb); // in: linear, out: srgb
    gl_FragColor = clamp(gl_FragColor, vec4(0.0), vec4(1.0));

    #endif // NOT RENDER_SHADOW
}

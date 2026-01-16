// https://google.github.io/filament/Filament.md.html

// http://www.mikktspace.com/
vec3 apply_normal_mapping(sampler2D normal_tex, vec3 ws_normal, vec4 ws_tangent, vec2 uv, bool flip_normal_map_y, bool double_sided) {
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

float distance_attenuation(float dist, float range) {
    float distanceSquare = dist * dist;
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
vec3 calculate_F0(vec3 base_color, float metallic, vec3 reflectance) {
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
vec2 F_AB(float perceptual_roughness, float NoV) {
    vec4 c0 = vec4(-1.0, -0.0275, -0.572, 0.022);
    vec4 c1 = vec4(1.0, 0.0425, 1.04, -0.04);
    vec4 r = perceptual_roughness * c0 + c1;
    float a004 = min(r.x * r.x, exp2(-9.28 * NoV)) * r.x + r.y;
    return vec2(-1.04, 1.04) * a004 + r.zw;
}


vec3 directional_light(vec3 V, vec3 F0, vec3 base_color, vec3 normal, float roughness, float shadow, vec3 light_dir, vec3 color) {
    vec3 res = vec3(0.0, 0.0, 0.0);
    if (shadow > 0.0 && light_dir != vec3(0.0)) {
        vec3 L = normalize(-light_dir);
        vec3 half_dir = normalize(L + V);
        float NoL = clamp(dot(normal, L), 0.0, 1.0);
        res += shadow * base_color.rgb * Fd_Lambert() * NoL * color;
        res += shadow * specular_brdf(V, L, normal, roughness, F0) * NoL * color;
    }
    return res;
}

vec3 environment_light(float NoV, vec3 F0, float perceptual_roughness, vec3 base_color, vec3 env_diffuse, vec3 env_specular) {
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
    vec3 k_D = base_color * (1.0 - FssEss - FmsEms);

    return ((FmsEms + k_D) * env_diffuse) + (FssEss * env_specular);
}

vec3 point_light(vec3 V, vec3 base_color, vec3 F0, vec3 normal, float roughness, vec3 to_light, float range, vec3 color, vec3 spot_dir, float spot_offset, float spot_scale) {
    vec3 res = vec3(0.0);
    float dist = length(to_light);
    if (dist < range) {
        vec3 L = normalize(to_light);
        float spot_attenuation = spot_angle_attenuation(spot_dir, L, spot_offset, spot_scale);
        float dist_attenuation = distance_attenuation(dist, range);

        float attenuation = dist_attenuation * spot_attenuation;
        float NoL = clamp(dot(normal, L), 0.0, 1.0);

        res += base_color.rgb * Fd_Lambert() * NoL * attenuation * color;
        res += specular_brdf(V, L, normal, roughness, F0) * NoL * attenuation * color;
    }
    return res;
}
varying vec3 ws_position;
varying vec4 tangent;
varying vec3 normal;
varying vec2 uv_0;
varying vec2 uv_1;

uniform vec3 view_position;

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

// https://github.com/bWFuanVzYWth/AgX/blob/0796e1b4aa9df94152eff353bae131eae1a4c087/agx.glsl
vec3 agx_tonemapping(vec3 /*Linear BT.709*/ ci) {
    const float min_ev = -12.473931188332413;
    const float max_ev = 4.026068811667588;
    const float dynamic_range = max_ev - min_ev;
    const mat3 agx_mat = mat3(0.8424010709504686, 0.04240107095046854, 0.04240107095046854, 0.07843650156180276, 0.8784365015618028, 0.07843650156180276, 0.0791624274877287, 0.0791624274877287, 0.8791624274877287);
    const mat3 agx_mat_inv = mat3(1.1969986613119143, -0.053001338688085674, -0.053001338688085674, -0.09804562695225345, 1.1519543730477466, -0.09804562695225345, -0.09895303435966087, -0.09895303435966087, 1.151046965640339);
    const float threshold = 0.6060606060606061;
    const float a_up = 69.86278913545539;
    const float a_down = 59.507875;
    const float b_up = 13.0 / 4.0;
    const float b_down = 3.0 / 1.0;
    const float c_up = -4.0 / 13.0;
    const float c_down = -1.0 / 3.0;
    ci = agx_mat * ci; // Input transform (inset)
    vec3 ct = clamp(log2(ci) * (1.0 / dynamic_range) - (min_ev / dynamic_range), 0.0, 1.0); // Apply sigmoid function
    vec3 mask = step(ct, vec3(threshold));
    vec3 a = a_up + (a_down - a_up) * mask;
    vec3 b = b_up + (b_down - b_up) * mask;
    vec3 c = c_up + (c_down - c_up) * mask;
    vec3 co = 0.5 + (((-2.0 * threshold)) + 2.0 * ct) * pow(1.0 + a * pow(abs(ct - threshold), b), c);
    co = agx_mat_inv * co; // Inverse input transform (outset)
    return /*BT.709 (NOT linear)*/ co;
}

void main() {
    vec4 color = base_color * texture2D(base_color_texture, uv_0);

    if (!alpha_blend && (color.a < 0.5)) {
        discard;
    }

    #ifndef DEPTH_PREPASS
    vec3 light_dir = normalize(vec3(-0.2, 0.5, 1.0));
    vec3 light_color = vec3(1.0, 0.9, 0.8) * 3.0;
    float specular_intensity = 1.0;

    vec3 V = normalize(ws_position - view_position);
    vec3 view_dir = normalize(view_position - ws_position);

    vec4 metallic_roughness = texture2D(metallic_roughness_texture, uv_0);
    float roughness = metallic_roughness.g * perceptual_roughness;
    roughness *= roughness;

    vec3 normal = apply_normal_mapping(normal, tangent, uv_0);

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
    #endif
}

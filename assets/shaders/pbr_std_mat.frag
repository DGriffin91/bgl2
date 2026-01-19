#include math
#include pbr
#include agx
#include std_mat_bindings
#include shadow_sampling
#include standard_pbr_lighting

void main() {
    vec4 base_color = base_color * to_linear(texture2D(base_color_texture, uv_0));

    if (!alpha_blend && (base_color.a < 0.5)) {
        discard;
    }
    if (write_reflection) {
        if (dot(ws_position - reflection_plane_position, reflection_plane_normal) < 0.0) {
            discard;
        }
    }

    vec3 ndc_position = clip_position.xyz / clip_position.w;
    vec2 screen_uv = ndc_position.xy * 0.5 + 0.5;

    #ifdef RENDER_DEPTH_ONLY
    gl_FragColor = EncodeFloatRGBA(saturate(ndc_position.z * 0.5 + 0.5));
    #else // RENDER_DEPTH_ONLY

    vec3 V = normalize(view_position - ws_position);

    vec4 metallic_roughness = texture2D(metallic_roughness_texture, uv_0);
    float perceptual_roughness = metallic_roughness.g * perceptual_roughness;
    float roughness = perceptual_roughness * perceptual_roughness;
    float metallic = metallic * metallic_roughness.b;
    vec3 F0 = calculate_F0(base_color.rgb, metallic, reflectance);
    vec3 diffuse_color = base_color.rgb * (1.0 - metallic);

    float emissive_exposure_factor = 1000.0; // TODO do something better
    vec3 emissive = emissive_exposure_factor * emissive.rgb * to_linear(texture2D(emissive_texture, uv_0).rgb);

    vec3 normal = vert_normal;
    if (has_normal_map) {
        normal = apply_normal_mapping(normal_map_texture, vert_normal, tangent, uv_0, flip_normal_map_y, double_sided);
    }

    vec3 output_color = apply_pbr_lighting(V, diffuse_color, F0, normal, roughness, diffuse_transmission, screen_uv);

    // TODO return struct from standard_lighting so the env map can be properly replaced by reflection?
    if (read_reflection && perceptual_roughness < 0.2) {
        vec3 sharp_reflection_color = reversible_tonemap_invert(texture2D(reflect_texture, screen_uv).rgb);
        output_color += sharp_reflection_color.rgb / view_exposure; // TODO integrate brdf properly
    }

    gl_FragColor = vec4(view_exposure * (output_color + emissive.rgb), base_color.a);
    if (write_reflection) {
        gl_FragColor.rgb = reversible_tonemap(gl_FragColor.rgb);
    } else {
        gl_FragColor.rgb = agx_tonemapping(gl_FragColor.rgb); // in: linear, out: srgb
        //gl_FragColor.rgb = from_linear(gl_FragColor.rgb); // in: linear, out: srgb
    }
    gl_FragColor = clamp(gl_FragColor, vec4(0.0), vec4(1.0));

    #endif // NOT RENDER_DEPTH_ONLY
}

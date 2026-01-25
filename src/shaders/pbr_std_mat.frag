#include std::math
#include std::pbr
#include std::agx
#include std::shadow_sampling
#include std::pbr_lighting

varying vec4 clip_position;
varying vec3 ws_position;
varying vec4 tangent;
varying vec3 vert_normal;
varying vec2 uv_0;

uniform sampler2D reflect_texture;
uniform bool read_reflection;
uniform bool write_reflection;
uniform vec3 reflection_plane_position;
uniform vec3 reflection_plane_normal;

void main() {
    vec4 base_color = ub_base_color * to_linear(texture2D(ub_base_color_texture, uv_0));

    if (!ub_alpha_blend && (base_color.a < 0.5)) {
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

    vec4 metallic_roughness = texture2D(ub_metallic_roughness_texture, uv_0);
    float perceptual_roughness = metallic_roughness.g * ub_perceptual_roughness;
    float metallic = ub_metallic * metallic_roughness.b;
    vec3 F0 = calculate_F0(base_color.rgb, metallic, ub_reflectance);
    vec3 diffuse_color = base_color.rgb * (1.0 - metallic);

    float emissive_exposure_factor = 1000.0; // TODO do something better
    vec3 emissive = emissive_exposure_factor * ub_emissive.rgb * to_linear(texture2D(ub_emissive_texture, uv_0).rgb);

    vec3 normal = vert_normal;
    if (ub_has_normal_map) {
        normal = apply_normal_mapping(ub_normal_map_texture, vert_normal, tangent, uv_0, ub_flip_normal_map_y, ub_double_sided);
    }

    vec3 output_color = emissive.rgb;
    float env_occ = 1.0;

    // TODO return struct from standard_lighting so the env map can be properly replaced by reflection?
    if (read_reflection && perceptual_roughness < 0.2) {
        vec3 sharp_reflection_color = reversible_tonemap_invert(texture2D(reflect_texture, screen_uv).rgb);
        output_color += sharp_reflection_color.rgb / view_exposure; // TODO integrate brdf properly
        env_occ = 0.0;
    }

    output_color += apply_pbr_lighting(V, diffuse_color, F0, vert_normal, normal, perceptual_roughness,
                                       env_occ, ub_diffuse_transmission, screen_uv, view_resolution, ws_position);

    gl_FragColor = vec4(view_exposure * output_color, base_color.a);
    if (write_reflection) {
        gl_FragColor.rgb = reversible_tonemap(gl_FragColor.rgb);
    } else {
        gl_FragColor.rgb = agx_tonemapping(gl_FragColor.rgb); // in: linear, out: srgb
        //gl_FragColor.rgb = from_linear(gl_FragColor.rgb); // in: linear, out: srgb
    }
    gl_FragColor = clamp(gl_FragColor, vec4(0.0), vec4(1.0));

    #endif // NOT RENDER_DEPTH_ONLY
}

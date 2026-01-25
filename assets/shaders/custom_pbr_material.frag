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

void main() {
    vec3 ndc_position = clip_position.xyz / clip_position.w;
    vec2 screen_uv = ndc_position.xy * 0.5 + 0.5;

    #ifdef RENDER_DEPTH_ONLY
    gl_FragColor = EncodeFloatRGBA(saturate(ndc_position.z * 0.5 + 0.5));
    #else // RENDER_DEPTH_ONLY

    vec4 base_color = texture2D(ub_color_texture, clip_position.xy);

    vec3 V = normalize(view_position - ws_position);

    vec3 normal = vert_normal;
    vec3 F0 = calculate_F0(base_color.rgb, ub_metallic, vec3(0.5));
    vec3 diffuse_color = base_color.rgb * (1.0 - ub_metallic);

    vec3 output_color = apply_pbr_lighting(V, diffuse_color, F0, vert_normal, normal, ub_perceptual_roughness,
            1.0, 0.0, screen_uv, view_resolution, ws_position);

    gl_FragColor = vec4(view_exposure * output_color, base_color.a);
    gl_FragColor.rgb = agx_tonemapping(gl_FragColor.rgb); // in: linear, out: srgb
    gl_FragColor = clamp(gl_FragColor, vec4(0.0), vec4(1.0));

    #endif //RENDER_DEPTH_ONLY
}

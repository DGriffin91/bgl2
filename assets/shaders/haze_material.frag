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
    //vec3 ndc_position = clip_position.xyz / clip_position.w;
    //vec2 screen_uv = ndc_position.xy * 0.5 + 0.5;
    //
    ////#ifdef RENDER_DEPTH_ONLY
    ////gl_FragColor = EncodeFloatRGBA(saturate(ndc_position.z * 0.5 + 0.5));
    ////#else // RENDER_DEPTH_ONLY
    //
    //vec4 base_color = ub_haze_color;
    //
    //vec3 V = normalize(ub_view_position - ws_position);
    //
    //vec3 normal = vert_normal;
    //
    //float alpha = uv_0.x;
    //
    //gl_FragColor = vec4(base_color.rgb, alpha);
    //gl_FragColor.rgb = agx_tonemapping(gl_FragColor.rgb); // in: linear, out: srgb
    //gl_FragColor = clamp(gl_FragColor, vec4(0.0), vec4(1.0));
    gl_FragColor = vec4(1.0, 0.0, 1.0, 1.0);

    //#endif //RENDER_DEPTH_ONLY
}

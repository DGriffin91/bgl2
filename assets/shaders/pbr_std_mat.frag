#include math
#include pbr
#include agx
#include std_mat_bindings
#include shadow_sampling

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

    float dir_shadow = 1.0;
    #ifdef SAMPLE_SHADOW
    float bias = 0.002;
    float normal_bias = 0.05;
    vec4 shadow_clip = shadow_clip_from_world * vec4(ws_position + vert_normal * normal_bias, 1.0);
    vec3 shadow_uvz = (shadow_clip.xyz / shadow_clip.w) * 0.5 + 0.5;

    if (shadow_uvz.x > 0.0 && shadow_uvz.x < 1.0 && shadow_uvz.y > 0.0 && shadow_uvz.y < 1.0) {
        dir_shadow *= bilinear_shadow2(shadow_texture, shadow_uvz.xy, shadow_uvz.z, bias, view_resolution);
        //dir_shadow *= sample_shadow_map_castano_thirteen(shadow_texture, shadow_uvz.xy, shadow_uvz.z, bias, view_resolution);
    }
    #endif // SAMPLE_SHADOW

    vec3 V = normalize(view_position - ws_position);

    vec4 metallic_roughness = texture2D(metallic_roughness_texture, uv_0);
    float perceptual_roughness = metallic_roughness.g * perceptual_roughness;
    float roughness = perceptual_roughness * perceptual_roughness;
    float metallic = metallic * metallic_roughness.b;
    vec3 F0 = calculate_F0(base_color.rgb, metallic, reflectance);
    base_color.rgb = base_color.rgb * (1.0 - metallic);

    float emissive_exposure_factor = 1000.0; // TODO do something better
    vec3 emissive = emissive_exposure_factor * emissive.rgb * to_linear(texture2D(emissive_texture, uv_0).rgb);

    vec3 normal = vert_normal;
    if (has_normal_map) {
        normal = apply_normal_mapping(normal_map_texture, vert_normal, tangent, uv_0, flip_normal_map_y, double_sided);
    }
    float NoV = abs(dot(normal, V)) + 1e-5;

    vec3 output_color = vec3(0.0);

    output_color += directional_light(V, F0, base_color.rgb, normal, roughness, dir_shadow, directional_light_dir, directional_light_color);


    {
        // Environment map / reflection
        float mip_levels = 8.0; // TODO put in uniform

        vec3 env_diffuse = rgbe2rgb(textureCubeLod(diffuse_map, vec3(normal.xy, -normal.z), 0.0)) * env_intensity;

        vec3 env_specular = vec3(0.0);
        if (read_reflection && perceptual_roughness < 0.2) {
            vec3 sharp_reflection_color = to_linear(texture2D(reflect_texture, screen_uv).rgb);
            // TODO integrate properly (invert tonemapping? blend post tonemapping?)
            output_color += sharp_reflection_color.rgb / view_exposure; 
        } else {
            vec3 dir = reflect(-V, normal);
            env_specular = rgbe2rgb(textureCubeLod(specular_map, vec3(dir.xy, -dir.z), perceptual_roughness * mip_levels)) * env_intensity;
        }

        output_color += environment_light(NoV, F0, perceptual_roughness, base_color.rgb, env_diffuse, env_specular);
    }

    // Point Lights
    for (int i = 0; i < MAX_POINT_LIGHTS; i++) {
        if (i < light_count) {
            vec4 light_position_range = point_light_position_range[i];
            vec3 to_light = light_position_range.xyz - ws_position;
            if (length(to_light) < light_position_range.w) {
                vec4 light_color_radius = point_light_color_radius[i];
                vec4 dos = spot_light_dir_offset_scale[i];
                output_color += point_light(V, base_color.rgb, F0, normal, roughness, to_light, light_position_range.w, 
                                            light_color_radius.rgb, octahedral_decode(dos.xy), dos.z, dos.w);
            }
        }
    }

    gl_FragColor = vec4(view_exposure * (output_color + emissive.rgb), base_color.a);
    gl_FragColor.rgb = agx_tonemapping(gl_FragColor.rgb); // in: linear, out: srgb
    //gl_FragColor.rgb = from_linear(gl_FragColor.rgb); // in: linear, out: srgb
    gl_FragColor = clamp(gl_FragColor, vec4(0.0), vec4(1.0));

    #endif // NOT RENDER_DEPTH_ONLY
}

uniform sampler2D ub_shadow_texture;

uniform mat4 ub_shadow_clip_from_world;
uniform vec3 ub_directional_light_dir;
uniform vec3 ub_directional_light_color;

uniform samplerCube ub_specular_map;
uniform samplerCube ub_diffuse_map;
uniform float ub_env_intensity;

uniform int ub_light_count;
uniform vec4 ub_point_light_position_range[MAX_POINT_LIGHTS];
uniform vec4 ub_point_light_color_radius[MAX_POINT_LIGHTS];
uniform vec4 ub_spot_light_dir_offset_scale[MAX_POINT_LIGHTS];

vec3 apply_pbr_lighting(vec3 V, vec3 diffuse_color, vec3 F0, vec3 vert_normal, vec3 normal, float perceptual_roughness,
    float diffuse_transmission, vec2 screen_uv, vec2 view_resolution, vec3 ws_position) {
    float roughness = perceptual_roughness * perceptual_roughness;
    vec3 output_color = vec3(0.0);

    float dir_shadow = 1.0;
    #ifdef SAMPLE_SHADOW
    float bias = 0.002;
    float normal_bias = 0.05;
    vec4 shadow_clip = ub_shadow_clip_from_world * vec4(ws_position + vert_normal * normal_bias, 1.0);
    vec3 shadow_uvz = (shadow_clip.xyz / shadow_clip.w) * 0.5 + 0.5;

    if (shadow_uvz.x > 0.0 && shadow_uvz.x < 1.0 && shadow_uvz.y > 0.0 && shadow_uvz.y < 1.0) {
        dir_shadow *= bilinear_shadow2(ub_shadow_texture, shadow_uvz.xy, shadow_uvz.z, bias, view_resolution);
        //dir_shadow *= sample_shadow_map_castano_thirteen(ub_shadow_texture, shadow_uvz.xy, shadow_uvz.z, bias, view_resolution);
        dir_shadow = hardenedKernel(dir_shadow);
    }
    #endif // SAMPLE_SHADOW

    output_color += directional_light(V, F0, diffuse_color, normal, roughness, diffuse_transmission, dir_shadow, ub_directional_light_dir, ub_directional_light_color);

    float NoV = abs(dot(normal, V)) + 1e-5;

    {
        // Environment map
        float mip_levels = 8.0; // TODO put in uniform
        vec3 dir = reflect(-V, normal);
        vec3 env_diffuse = rgbe2rgb(textureCubeLod(ub_diffuse_map, vec3(normal.xy, -normal.z), 0.0)) * ub_env_intensity;
        vec3 env_specular = rgbe2rgb(textureCubeLod(ub_specular_map, vec3(dir.xy, -dir.z), perceptual_roughness * mip_levels)) * ub_env_intensity;
        output_color += environment_light(NoV, F0, perceptual_roughness, diffuse_color, env_diffuse, env_specular);
    }

    #ifndef NO_POINT
    // Point Lights
    for (int i = 0; i < MAX_POINT_LIGHTS; i++) {
        if (i < ub_light_count) {
            vec4 light_position_range = ub_point_light_position_range[i];
            vec3 to_light = light_position_range.xyz - ws_position;
            if (length(to_light) < light_position_range.w) {
                vec4 light_color_radius = ub_point_light_color_radius[i];
                vec4 dos = ub_spot_light_dir_offset_scale[i];
                vec3 spot_dir = octahedral_decode(dos.xy);
                output_color += point_light(V, diffuse_color, F0, normal, roughness, diffuse_transmission, to_light,
                        light_position_range.w, light_color_radius.rgb, spot_dir, dos.z, dos.w);
            }
        }
    }
    #endif

    return output_color;
}

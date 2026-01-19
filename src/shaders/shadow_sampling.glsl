float bilinear_shadow(sampler2D shadow_tex, vec2 uv, float receiver_z, float bias, vec2 shadow_res) {
    vec2 step = 1.0 / shadow_res;

    vec2 p = (uv * shadow_res - 0.5);
    vec2 pos = floor(p) * step;
    vec2 f = fract(p);

    float t00 = float(receiver_z > DecodeFloatRGBA(texture2D(shadow_tex, pos + vec2(0.0, 0.0))) - bias);
    float t10 = float(receiver_z > DecodeFloatRGBA(texture2D(shadow_tex, pos + vec2(step.x, 0.0))) - bias);
    float t01 = float(receiver_z > DecodeFloatRGBA(texture2D(shadow_tex, pos + vec2(0.0, step.y))) - bias);
    float t11 = float(receiver_z > DecodeFloatRGBA(texture2D(shadow_tex, pos + vec2(step.x, step.y))) - bias);

    return mix(mix(t00, t10, f.x), mix(t01, t11, f.x), f.y);
}

float bilinear_shadow2(sampler2D shadow_tex, vec2 uv, float receiver_z, float bias, vec2 shadow_res) {
    vec2 step = 1.0 / shadow_res;

    vec2 p = (uv * shadow_res - 0.5);
    vec2 pos = floor(p) * step;
    vec2 f = fract(p);

    float t00 = bilinear_shadow(shadow_tex, pos + vec2(0.0, 0.0), receiver_z, bias, shadow_res);
    float t10 = bilinear_shadow(shadow_tex, pos + vec2(step.x, 0.0), receiver_z, bias, shadow_res);
    float t01 = bilinear_shadow(shadow_tex, pos + vec2(0.0, step.y), receiver_z, bias, shadow_res);
    float t11 = bilinear_shadow(shadow_tex, pos + vec2(step.x, step.y), receiver_z, bias, shadow_res);

    return mix(mix(t00, t10, f.x), mix(t01, t11, f.x), f.y);
}

float bilinear_shadow_cont(sampler2D shadow_tex, vec2 uv, float receiver_z, float bias, vec2 shadow_res) {
    vec2 step = 1.0 / shadow_res;

    vec2 p = (uv * shadow_res - 0.5);
    vec2 pos = floor(p) * step;
    vec2 f = fract(p);

    float t00 = DecodeFloatRGBA(texture2D(shadow_tex, pos + vec2(0.0, 0.0)));
    float t10 = DecodeFloatRGBA(texture2D(shadow_tex, pos + vec2(step.x, 0.0)));
    float t01 = DecodeFloatRGBA(texture2D(shadow_tex, pos + vec2(0.0, step.y)));
    float t11 = DecodeFloatRGBA(texture2D(shadow_tex, pos + vec2(step.x, step.y)));

    float result = mix(mix(t00, t10, f.x), mix(t01, t11, f.x), f.y);

    return float(receiver_z > result - bias);
}

float sample_shadow_map_castano_thirteen(sampler2D shadow_tex, vec2 uv, float receiver_z, float bias, vec2 shadow_res) {
    vec2 inv_map_size = vec2(1.0) / shadow_res;

    uv = uv * shadow_res;
    vec2 base_uv = floor(uv + vec2(0.5));
    float s = (uv.x + 0.5 - base_uv.x);
    float t = (uv.y + 0.5 - base_uv.y);
    base_uv -= 0.5;
    base_uv *= inv_map_size;

    float uw0 = (4.0 - 3.0 * s);
    float uw1 = 7.0;
    float uw2 = (1.0 + 3.0 * s);

    float u0 = (3.0 - 2.0 * s) / uw0 - 2.0;
    float u1 = (3.0 + s) / uw1;
    float u2 = s / uw2 + 2.0;

    float vw0 = (4.0 - 3.0 * t);
    float vw1 = 7.0;
    float vw2 = (1.0 + 3.0 * t);

    float v0 = (3.0 - 2.0 * t) / vw0 - 2.0;
    float v1 = (3.0 + t) / vw1;
    float v2 = t / vw2 + 2.0;

    float sum = 0.0;

    sum += uw0 * vw0 * bilinear_shadow(shadow_tex, base_uv + vec2(u0, v0) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw1 * vw0 * bilinear_shadow(shadow_tex, base_uv + vec2(u1, v0) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw2 * vw0 * bilinear_shadow(shadow_tex, base_uv + vec2(u2, v0) * inv_map_size, receiver_z, bias, shadow_res);

    sum += uw0 * vw1 * bilinear_shadow(shadow_tex, base_uv + vec2(u0, v1) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw1 * vw1 * bilinear_shadow(shadow_tex, base_uv + vec2(u1, v1) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw2 * vw1 * bilinear_shadow(shadow_tex, base_uv + vec2(u2, v1) * inv_map_size, receiver_z, bias, shadow_res);

    sum += uw0 * vw2 * bilinear_shadow(shadow_tex, base_uv + vec2(u0, v2) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw1 * vw2 * bilinear_shadow(shadow_tex, base_uv + vec2(u1, v2) * inv_map_size, receiver_z, bias, shadow_res);
    sum += uw2 * vw2 * bilinear_shadow(shadow_tex, base_uv + vec2(u2, v2) * inv_map_size, receiver_z, bias, shadow_res);

    return sum / 144.0;
}

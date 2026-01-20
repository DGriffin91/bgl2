#define PI 3.14159274

float saturate(float v) {
    return clamp(v, 0.0, 1.0);
}

vec2 saturate(vec2 v) {
    return clamp(v, 0.0, 1.0);
}

vec3 saturate(vec3 v) {
    return clamp(v, 0.0, 1.0);
}

vec4 saturate(vec4 v) {
    return clamp(v, 0.0, 1.0);
}

// For decoding normals or unit direction vectors from octahedral coordinates.
vec3 octahedral_decode(vec2 v) {
    vec2 f = v * 2.0 - 1.0;
    vec3 n = vec3(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));
    float t = saturate(-n.z);
    vec2 w = vec2(t);
    if (n.x > 0.0) {
        w.x = -w.x;
    }
    if (n.y > 0.0) {
        w.y = -w.y;
    }
    return normalize(vec3(n.x + w.x, n.y + w.y, n.z));
}

// https://aras-p.info/blog/2009/07/30/encoding-floats-to-rgba-the-final/
vec4 EncodeFloatRGBA(float v) {
    vec4 enc = vec4(1.0, 255.0, 65025.0, 16581375.0) * saturate(v);
    enc = fract(enc);
    enc -= enc.yzww * vec4(1.0 / 255.0, 1.0 / 255.0, 1.0 / 255.0, 0.0);
    return enc;
}

float DecodeFloatRGBA(vec4 rgba) {
    return saturate(dot(rgba, vec4(1.0, 1.0 / 255.0, 1.0 / 65025.0, 1.0 / 16581375.0)));
}

vec3 rgbe2rgb(vec4 rgbe) {
    return (rgbe.rgb * exp2(rgbe.a * 255.0 - 128.0) * 0.99609375); // (255.0/256.0)
}

vec3 to_linear(vec3 sRGB) {
    return pow(sRGB, vec3(2.2));
}

vec4 to_linear(vec4 sRGB) {
    return vec4(pow(sRGB.rgb, vec3(2.2)), sRGB.a);
}

vec3 from_linear(vec3 linearRGB) {
    return pow(linearRGB, vec3(1.0 / 2.2));
}

vec4 from_linear(vec4 linearRGB) {
    return vec4(pow(linearRGB.rgb, vec3(1.0 / 2.2)), linearRGB.a);
}

float max3(float x, float y, float z) { return max(x, max(y, z)); }

// https://gpuopen.com/learn/optimized-reversible-tonemapper-for-resolve/
// Apply this to tonemap linear HDR color "c" after a sample is fetched in the resolve.
// Note "c" 1.0 maps to the expected limit of low-dynamic-range monitor output.
vec3 reversible_tonemap(vec3 c) { return c * (1.0 / (max3(c.r, c.g, c.b) + 1.0)); }

// When the filter kernel is a weighted sum of fetched colors,
// it is more optimal to fold the weighting into the tonemap operation.
vec3 reversible_tonemap_weighted(vec3 c, float w) { return c * (w * (1.0 / (max3(c.r, c.g, c.b) + 1.0))); }

// Apply this to restore the linear HDR color before writing out the result of the resolve.
vec3 reversible_tonemap_invert(vec3 c) { return c * (1.0 / (1.0 - max3(c.r, c.g, c.b))); }
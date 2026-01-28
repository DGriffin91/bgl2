#define PI 3.14159274
#define REC709_PRIMARIES vec3(0.2126, 0.7152, 0.0722)

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

float max3(float x, float y, float z) {
    return max(x, max(y, z));
}

// https://gpuopen.com/learn/optimized-reversible-tonemapper-for-resolve/
// Apply this to tonemap linear HDR color "c" after a sample is fetched in the resolve.
// Note "c" 1.0 maps to the expected limit of low-dynamic-range monitor output.
vec3 reversible_tonemap(vec3 c) {
    return c * (1.0 / (max3(c.r, c.g, c.b) + 1.0));
}

// When the filter kernel is a weighted sum of fetched colors,
// it is more optimal to fold the weighting into the tonemap operation.
vec3 reversible_tonemap_weighted(vec3 c, float w) {
    return c * (w * (1.0 / (max3(c.r, c.g, c.b) + 1.0)));
}

// Apply this to restore the linear HDR color before writing out the result of the resolve.
vec3 reversible_tonemap_invert(vec3 c) {
    return c * (1.0 / (1.0 - max3(c.r, c.g, c.b)));
}

vec4 cubic(float v) {
    vec4 n = vec4(1.0, 2.0, 3.0, 4.0) - v;
    vec4 s = n * n * n;
    float x = s.x;
    float y = s.y - 4.0 * s.x;
    float z = s.z - 4.0 * s.y + 6.0 * s.x;
    float w = 6.0 - x - y - z;
    return vec4(x, y, z, w) * (1.0 / 6.0);
}

vec4 textureBicubic(sampler2D sampler, vec2 texCoords, vec2 texSize) {
    vec2 invTexSize = 1.0 / texSize;

    texCoords = texCoords * texSize - 0.5;

    vec2 fxy = fract(texCoords);
    texCoords -= fxy;

    vec4 xcubic = cubic(fxy.x);
    vec4 ycubic = cubic(fxy.y);

    vec4 c = texCoords.xxyy + vec2(-0.5, +1.5).xyxy;

    vec4 s = vec4(xcubic.xz + xcubic.yw, ycubic.xz + ycubic.yw);
    vec4 offset = c + vec4(xcubic.yw, ycubic.yw) / s;

    offset *= invTexSize.xxyy;

    vec4 sample0 = texture2D(sampler, offset.xz);
    vec4 sample1 = texture2D(sampler, offset.yz);
    vec4 sample2 = texture2D(sampler, offset.xw);
    vec4 sample3 = texture2D(sampler, offset.yw);

    float sx = s.x / (s.x + s.y);
    float sy = s.z / (s.z + s.w);

    return mix(mix(sample3, sample2, sx), mix(sample1, sample0, sx), sy);
}

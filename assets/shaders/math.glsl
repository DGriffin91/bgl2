// For decoding normals or unit direction vectors from octahedral coordinates.
vec3 octahedral_decode(vec2 v) {
    vec2 f = v * 2.0 - 1.0;
    vec3 n = vec3(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));
    float t = clamp(-n.z, 0.0, 1.0);
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
    vec4 enc = vec4(1.0, 255.0, 65025.0, 16581375.0) * clamp(v, 0.0, 1.0);
    enc = fract(enc);
    enc -= enc.yzww * vec4(1.0 / 255.0, 1.0 / 255.0, 1.0 / 255.0, 0.0);
    return enc;
}

float DecodeFloatRGBA(vec4 rgba) {
    return clamp(dot(rgba, vec4(1.0, 1.0 / 255.0, 1.0 / 65025.0, 1.0 / 16581375.0)), 0.0, 1.0);
}

vec3 rgbe2rgb(vec4 rgbe) {
    return (rgbe.rgb * pow(2.0, rgbe.a * 255.0 - 128.0));
}
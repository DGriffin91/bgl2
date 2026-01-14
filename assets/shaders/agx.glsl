// https://github.com/bWFuanVzYWth/AgX/blob/0796e1b4aa9df94152eff353bae131eae1a4c087/agx.glsl
vec3 agx_tonemapping(vec3 /*Linear BT.709*/ ci) {
    const float min_ev = -12.473931188332413;
    const float max_ev = 4.026068811667588;
    const float dynamic_range = max_ev - min_ev;
    const mat3 agx_mat = mat3(0.8424010709504686, 0.04240107095046854, 0.04240107095046854, 0.07843650156180276, 0.8784365015618028, 0.07843650156180276, 0.0791624274877287, 0.0791624274877287, 0.8791624274877287);
    const mat3 agx_mat_inv = mat3(1.1969986613119143, -0.053001338688085674, -0.053001338688085674, -0.09804562695225345, 1.1519543730477466, -0.09804562695225345, -0.09895303435966087, -0.09895303435966087, 1.151046965640339);
    const float threshold = 0.6060606060606061;
    const float a_up = 69.86278913545539;
    const float a_down = 59.507875;
    const float b_up = 13.0 / 4.0;
    const float b_down = 3.0 / 1.0;
    const float c_up = -4.0 / 13.0;
    const float c_down = -1.0 / 3.0;
    ci = agx_mat * ci; // Input transform (inset)
    vec3 ct = clamp(log2(ci) * (1.0 / dynamic_range) - (min_ev / dynamic_range), 0.0, 1.0); // Apply sigmoid function
    vec3 mask = step(ct, vec3(threshold));
    vec3 a = a_up + (a_down - a_up) * mask;
    vec3 b = b_up + (b_down - b_up) * mask;
    vec3 c = c_up + (c_down - c_up) * mask;
    vec3 co = 0.5 + (((-2.0 * threshold)) + 2.0 * ct) * pow(1.0 + a * pow(abs(ct - threshold), b), c);
    co = agx_mat_inv * co; // Inverse input transform (outset)
    return /*BT.709 (NOT linear)*/ co;
}

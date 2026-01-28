#include std::math

varying vec2 uv_0;

void main() {
    vec4 base_color = ub_haze_color;

    float x = uv_0.x;
    float y = uv_0.y;
    float a = saturate((1.0 - y) * 5.0);
    float b = saturate(y * 5.0);
    float c = saturate(1.0 - x);
    float d = saturate(x * 5.0);

    float alpha = a * b * c * d;
    alpha *= alpha;

    gl_FragColor = vec4(base_color.rgb, alpha * base_color.a);
    gl_FragColor.rgb = from_linear(gl_FragColor.rgb);
}

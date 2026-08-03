#version 300 es
// flat.frag: For enhanced target, no dithering, RGB888, also semi transparency by sampling the enhanced target texture
precision highp float;

uniform sampler2D vram;

in vec3 frag_colour;
flat in float frag_semi_transparent;
flat in float frag_semi_transparency_mode;

out vec4 out_colour;

void main() {
    vec3 out_rgb = clamp(frag_colour / 255.0, 0.0, 1.0);

    bool is_semi_transparent = frag_semi_transparent > 0.5;
    int mode = int(frag_semi_transparency_mode + 0.5);

    if (is_semi_transparent) {
        vec3 old = texelFetch(vram, ivec2(gl_FragCoord.xy), 0).rgb;

        if (mode == 0) {
            out_rgb = (old + out_rgb) / 2.0;
        } else if (mode == 1) {
            out_rgb = min(old + out_rgb, 1.0);
        } else if (mode == 2) {
            out_rgb = max(old - out_rgb, 0.0);
        } else {
            out_rgb = min(old + out_rgb / 4.0, 1.0);
        }
    }

    out_colour = vec4(out_rgb, 1.0);
}
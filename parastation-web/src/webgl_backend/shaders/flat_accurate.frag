#version 300 es
// flat_accurate.frag: Accurate target, dithering, RGB555
precision highp float;
precision highp int;
precision highp usampler2D;

uniform usampler2D vram;

in vec3 frag_colour;
flat in float frag_dither;
flat in float frag_semi_transparent;
flat in float frag_semi_transparency_mode;

layout(location = 0) out uint out_colour;

uint vram_read(ivec2 coord) {
    return texelFetch(vram, coord, 0).r;
}

// Dithering table - exact match to real PS1 hardware, verified against native backend
const int dither_table[16] = int[16](
    -4,  0, -3,  1,
     2, -2,  3, -1,
    -3,  1, -4,  0,
     3, -1,  2, -2
);

int get_dither(ivec2 coord) {
    int x = coord.x & 3;
    int y = coord.y & 3;
    return dither_table[y * 4 + x];
}

void main() {
    vec3 c = clamp(frag_colour, 0.0, 255.0);

    bool dither = frag_dither > 0.5;
    int dith = dither ? get_dither(ivec2(gl_FragCoord.xy)) : 0;

    int ir = clamp(int(c.r) + dith, 0, 255) >> 3;
    int ig = clamp(int(c.g) + dith, 0, 255) >> 3;
    int ib = clamp(int(c.b) + dith, 0, 255) >> 3;

    bool is_semi_transparent = frag_semi_transparent > 0.5;
    int mode = int(frag_semi_transparency_mode + 0.5);

    if (is_semi_transparent) {
        uint old = vram_read(ivec2(gl_FragCoord.xy));
        int br = int((old >>  0u) & 0x1Fu);
        int bg = int((old >>  5u) & 0x1Fu);
        int bb = int((old >> 10u) & 0x1Fu);

        if (mode == 0) {
            ir = (br + ir) / 2;
            ig = (bg + ig) / 2;
            ib = (bb + ib) / 2;
        } else if (mode == 1) {
            ir = min(br + ir, 31);
            ig = min(bg + ig, 31);
            ib = min(bb + ib, 31);
        } else if (mode == 2) {
            ir = max(br - ir, 0);
            ig = max(bg - ig, 0);
            ib = max(bb - ib, 0);
        } else {
            ir = min(br + ir / 4, 31);
            ig = min(bg + ig / 4, 31);
            ib = min(bb + ib / 4, 31);
        }
    }

    out_colour = uint(ir) | (uint(ig) << 5u) | (uint(ib) << 10u);
}
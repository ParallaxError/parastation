// FLAT_FRAG: Simple shader for flat-coloured polygons, converting from 0-255 scale to RGB555 for PS1 output
#version 330 core
uniform usampler2D vram;
uniform bool is_semi_transparent;
uniform int semi_transparency_mode;
uniform bool dither;

in vec3 frag_colour;
out uvec4 out_colour;

uint vram_read(ivec2 coord) {
    return texelFetch(vram, coord, 0).r;
}

// Dithering table
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
    vec3 c = clamp(frag_colour, 0.0, 1.0) * 255.0;

    int dith = dither ? get_dither(ivec2(gl_FragCoord.xy)) : 0;

    int ir = clamp(int(c.r) + dith, 0, 255) >> 3;
    int ig = clamp(int(c.g) + dith, 0, 255) >> 3;
    int ib = clamp(int(c.b) + dith, 0, 255) >> 3;

    // // Handle semi transparency: https://psx-spx.consoledev.net/graphicsprocessingunitgpu/#semi-transparency
    if (is_semi_transparent) {
        uint old = vram_read(ivec2(gl_FragCoord.xy));
        int br = int((old >>  0u) & 0x1Fu);
        int bg = int((old >>  5u) & 0x1Fu);
        int bb = int((old >> 10u) & 0x1Fu);

        if (semi_transparency_mode == 0) {
            ir = (br + ir) / 2;
            ig = (bg + ig) / 2;
            ib = (bb + ib) / 2;
        } else if (semi_transparency_mode == 1) {
            ir = min(br + ir, 31);
            ig = min(bg + ig, 31);
            ib = min(bb + ib, 31);
        } else if (semi_transparency_mode == 2) {
            ir = max(br - ir, 0);
            ig = max(bg - ig, 0);
            ib = max(bb - ib, 0);
        } else {
            ir = min(br + ir / 4, 31);
            ig = min(bg + ig / 4, 31);
            ib = min(bb + ib / 4, 31);
        }
    }

    uint bgr555 = uint(ir) | (uint(ig) << 5u) | (uint(ib) << 10u);
    out_colour = uvec4(bgr555, 0u, 0u, 0u);
}
// FLAT_FRAG: Simple shader for flat-coloured polygons, converting from 0-255 scale to RGB555 for PS1 output
#version 330 core
uniform usampler2D vram;
uniform bool is_semi_transparent;
uniform int semi_transparency_mode;

in vec3 frag_colour;
out uvec4 out_colour;

uint vram_read(ivec2 coord) {
    return texelFetch(vram, coord, 0).r;
}

void main() {
    // TODO: Dithering
    float cr = clamp(frag_colour.r, 0.0, 1.0);
    float cg = clamp(frag_colour.g, 0.0, 1.0);
    float cb = clamp(frag_colour.b, 0.0, 1.0);

    int ir = int(cr * 31.0 + 0.5);
    int ig = int(cg * 31.0 + 0.5);
    int ib = int(cb * 31.0 + 0.5);

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
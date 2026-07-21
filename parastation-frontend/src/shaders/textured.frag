// TEXTURED_FRAG: Shader for textured polygons handling PS1 colour depth, blending and transparency
#version 330 core
uniform usampler2D vram;
uniform int tex_depth;
uniform vec2 tex_page;
uniform bool is_semi_transparent;
uniform int semi_transparency_mode;
uniform bool is_raw_texture;
uniform bool dither;
uniform vec4 tex_window;

in vec3 frag_colour;
in vec2 frag_texcoord;
in vec2 frag_clut;

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
    ivec2 mask   = ivec2(tex_window.xy);
    ivec2 offset = ivec2(tex_window.zw);
    ivec2 tex_raw = ivec2(frag_texcoord);
    // Mask from the tex_window setting of the PS1
    ivec2 tex = (tex_raw & (~mask)) | (offset & mask);

    ivec2 clut = ivec2(frag_clut);
    ivec2 page = ivec2(tex_page);
    uint raw;

    // Handle CLUT lookup and texpage sampling in accordance with PS1 depth settings
    if (tex_depth == 0) {
        uint word = vram_read(ivec2(page.x + tex.x / 4, page.y + tex.y));
        uint index = (word >> (uint(tex.x % 4) * 4u)) & 0xFu;
        raw = vram_read(ivec2(clut.x + int(index), clut.y));
    }
    else if (tex_depth == 1) {
        uint word = vram_read(ivec2(page.x + tex.x / 2, page.y + tex.y));
        uint index = (word >> (uint(tex.x % 2) * 8u)) & 0xFFu;
        raw = vram_read(ivec2(clut.x + int(index), clut.y));
    }
    else {
        raw = vram_read(ivec2(page.x + tex.x, page.y + tex.y));
    }

    if (raw == 0x0000u) discard;

    // Get the components from the raw RGB555 value
    uint raw_r = (raw >>  0u) & 0x1Fu;
    uint raw_g = (raw >>  5u) & 0x1Fu;
    uint raw_b = (raw >> 10u) & 0x1Fu;
    bool clut_stp = (raw & 0x8000u) != 0u;

    int out_r, out_g, out_b;

    if (is_raw_texture) {
        out_r = int(raw_r);
        out_g = int(raw_g);
        out_b = int(raw_b);
    } else {
        // Modulate in 8-bit-equivalent space (raw_5bit * colour_8bit / 128 keeps result in 0-31, so scale up by 8 to 
        // dither properly
        int mod_r = int(min((raw_r * uint(frag_colour.r)) / 128u, 31u));
        int mod_g = int(min((raw_g * uint(frag_colour.g)) / 128u, 31u));
        int mod_b = int(min((raw_b * uint(frag_colour.b)) / 128u, 31u));

        int dith = dither ? get_dither(ivec2(gl_FragCoord.xy)) : 0;

        // dither table is scaled for 8-bit; approximate at 5-bit by dividing offset by 8, clamped to avoid rounding to 
        // zero losing effect entirely
        out_r = clamp(mod_r + dith / 8, 0, 31);
        out_g = clamp(mod_g + dith / 8, 0, 31);
        out_b = clamp(mod_b + dith / 8, 0, 31);
    }

    // Handle semi transparency: https://psx-spx.consoledev.net/graphicsprocessingunitgpu/#semi-transparency
    /*
    For textured primitives using 4-bit or 8-bit textures, bit 15 of each CLUT entry acts as a semi-transparency flag 
    and determines whether to apply semi-transparency to the pixel or not. If the semi-transparency flag is off, the new
     pixel is written to VRAM as-is
    */
    if (is_semi_transparent && clut_stp) {
        uint old = vram_read(ivec2(gl_FragCoord.xy));
        int br = int((old >>  0u) & 0x1Fu);
        int bg = int((old >>  5u) & 0x1Fu);
        int bb = int((old >> 10u) & 0x1Fu);

        if (semi_transparency_mode == 0) {
            out_r = (br + out_r) / 2;
            out_g = (bg + out_g) / 2;
            out_b = (bb + out_b) / 2;
        } else if (semi_transparency_mode == 1) {
            out_r = min(br + out_r, 31);
            out_g = min(bg + out_g, 31);
            out_b = min(bb + out_b, 31);
        } else if (semi_transparency_mode == 2) {
            out_r = max(br - out_r, 0);
            out_g = max(bg - out_g, 0);
            out_b = max(bb - out_b, 0);
        } else {
            out_r = min(br + out_r / 4, 31);
            out_g = min(bg + out_g / 4, 31);
            out_b = min(bb + out_b / 4, 31);
        }
    }

    // Repack to RGB555 for the PS1 output
    uint bgr555 = uint(out_r) | (uint(out_g) << 5u) | (uint(out_b) << 10u);
    out_colour = uvec4(bgr555, 0u, 0u, 0u);
}
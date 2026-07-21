// TEXTURED_FRAG: Shader for textured polygons handling PS1 colour depth, blending and transparency
#version 330 core
uniform usampler2D vram;
uniform int tex_depth;
uniform vec2 tex_page;
uniform bool is_semi_transparent;
uniform int semi_transparency_mode;
uniform bool is_raw_texture;
uniform vec4 tex_window;

in vec3 frag_colour;
in vec2 frag_texcoord;
in vec2 frag_clut;

out uvec4 out_colour;

uint vram_read(ivec2 coord) {
    return texelFetch(vram, coord, 0).r;
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

    // frag_colour is 0-255 scale, and so we scale it down to 0-2 for the RGB555 output, and multiply by the 
    // texture colour
    int out_r = int(is_raw_texture ? raw_r : min((raw_r * uint(frag_colour.r)) / 128u, 31u));
    int out_g = int(is_raw_texture ? raw_g : min((raw_g * uint(frag_colour.g)) / 128u, 31u));
    int out_b = int(is_raw_texture ? raw_b : min((raw_b * uint(frag_colour.b)) / 128u, 31u));

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
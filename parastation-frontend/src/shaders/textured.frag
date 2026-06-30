// TEXTURED_FRAG: Shader for textured polygons handling PS1 colour depth, blending and transparency
#version 330 core
uniform usampler2D vram;
uniform int tex_depth;
uniform vec2 tex_page;
uniform bool is_semi_transparent;
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

    // frag_colour is 0-255 scale, and so we scale it down to 0-31 for the RGB555 output, and multiply by the 
    // texture colour
    uint out_r = min((raw_r * uint(frag_colour.r)) / 128u, 31u);
    uint out_g = min((raw_g * uint(frag_colour.g)) / 128u, 31u);
    uint out_b = min((raw_b * uint(frag_colour.b)) / 128u, 31u);

    // Repack to RGB555 for the PS1 output
    uint bgr555 = out_r | (out_g << 5u) | (out_b << 10u);
    out_colour = uvec4(bgr555, 0u, 0u, 0u);
}
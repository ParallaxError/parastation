#version 330 core
uniform usampler2D vram;
uniform int tex_depth;
uniform vec2 tex_page;
uniform bool is_semi_transparent;

in vec3 frag_colour;
in vec2 frag_texcoord;
in vec2 frag_clut;

out vec4 out_colour;

uint vram_read(ivec2 coord) {
    return texelFetch(vram, coord, 0).r;
}

vec4 bgr555_to_rgba(uint c) {
    float r = float((c >>  0u) & 0x1Fu) / 31.0;
    float g = float((c >>  5u) & 0x1Fu) / 31.0;
    float b = float((c >> 10u) & 0x1Fu) / 31.0;
    return vec4(r, g, b, 1.0);
}

void main() {
    ivec2 tex  = ivec2(frag_texcoord);
    ivec2 clut = ivec2(frag_clut);
    ivec2 page = ivec2(tex_page);
    uint raw;

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

    vec4 texel = bgr555_to_rgba(raw);
    texel.a = 1.0;

    out_colour = texel * vec4(frag_colour / 128.0, 1.0);
}
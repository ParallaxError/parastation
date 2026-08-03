#version 300 es
// textured.frag: For the enhanced target, samples the 15 bit VRAM texture for textured polygons and samples the 
// enhanced target texture for semi transparent polygons
precision highp float;
precision highp int;
precision highp usampler2D;

uniform usampler2D vram;
uniform sampler2D enhanced_sample;

in vec3 frag_colour;
in vec2 frag_uv;

flat in vec2 frag_clut;
flat in vec2 frag_tex_page;
flat in float frag_tex_depth;
flat in vec4 frag_tex_window;
flat in float frag_is_raw_texture;
flat in float frag_semi_transparent;
flat in float frag_semi_transparency_mode;

out vec4 out_colour;

uint vram_read(ivec2 coord) {
    return texelFetch(vram, coord, 0).r;
}

void main() {
    ivec2 mask   = ivec2(frag_tex_window.xy);
    ivec2 offset = ivec2(frag_tex_window.zw);
    ivec2 tex_raw = ivec2(frag_uv);
    // Mask from the tex_window setting of the PS1
    ivec2 tex = (tex_raw & (~mask)) | (offset & mask);

    ivec2 clut = ivec2(frag_clut);
    ivec2 page = ivec2(frag_tex_page);
    int tex_depth = int(frag_tex_depth + 0.5);
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
    float raw_r = float((raw >>  0u) & 0x1Fu) / 31.0;
    float raw_g = float((raw >>  5u) & 0x1Fu) / 31.0;
    float raw_b = float((raw >> 10u) & 0x1Fu) / 31.0;
    bool clut_stp = (raw & 0x8000u) != 0u;

    bool is_raw_texture = frag_is_raw_texture > 0.5;

    vec3 out_rgb;
    if (is_raw_texture) {
        out_rgb = vec3(raw_r, raw_g, raw_b);
    } else {
        out_rgb = clamp(vec3(raw_r, raw_g, raw_b) * (frag_colour / 128.0), 0.0, 1.0);
    }

    bool is_semi_transparent = frag_semi_transparent > 0.5;
    int semi_transparency_mode = int(frag_semi_transparency_mode + 0.5);

    // Handle semi transparency: https://psx-spx.consoledev.net/graphicsprocessingunitgpu/#semi-transparency
    /*
    For textured primitives using 4-bit or 8-bit textures, bit 15 of each CLUT entry acts as a semi-transparency flag 
    and determines whether to apply semi-transparency to the pixel or not. If the semi-transparency flag is off, the new
     pixel is written to VRAM as-is
    */
    if (is_semi_transparent && clut_stp) {
        vec3 old_rgb = texelFetch(enhanced_sample, ivec2(gl_FragCoord.xy), 0).rgb;

        if (semi_transparency_mode == 0) {
            out_rgb = (old_rgb + out_rgb) / 2.0;
        } else if (semi_transparency_mode == 1) {
            out_rgb = min(old_rgb + out_rgb, 1.0);
        } else if (semi_transparency_mode == 2) {
            out_rgb = max(old_rgb - out_rgb, 0.0);
        } else {
            out_rgb = min(old_rgb + out_rgb / 4.0, 1.0);
        }
    }

    out_colour = vec4(out_rgb, 1.0);
}
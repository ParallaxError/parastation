#version 300 es
// textured.vert
precision highp float;

layout(location = 0) in vec2 position;
layout(location = 1) in vec3 colour;
layout(location = 2) in vec2 uv;
layout(location = 3) in vec2 clut;
layout(location = 4) in vec2 tex_page;
layout(location = 5) in float tex_depth;
layout(location = 6) in vec4 tex_window; // mask_x, mask_y, offset_x, offset_y
layout(location = 7) in float is_raw_texture;
layout(location = 8) in float dither;
layout(location = 9) in float semi_transparent;
layout(location = 10) in float semi_transparency_mode;

uniform float scale;

out vec3 frag_colour;
out vec2 frag_uv;

flat out vec2 frag_clut;
flat out vec2 frag_tex_page;
flat out float frag_tex_depth;
flat out vec4 frag_tex_window;
flat out float frag_is_raw_texture;
flat out float frag_dither;
flat out float frag_semi_transparent;
flat out float frag_semi_transparency_mode;

void main() {
    vec2 scaled_pos = position * scale;
    vec2 target_size = vec2(1024.0, 512.0) * scale;
    vec2 ndc = (scaled_pos / target_size) * 2.0 - 1.0;

    gl_Position = vec4(ndc, 0.0, 1.0);

    frag_colour = colour;
    frag_uv = uv;
    frag_clut = clut;
    frag_tex_page = tex_page;
    frag_tex_depth = tex_depth;
    frag_tex_window = tex_window;
    frag_is_raw_texture = is_raw_texture;
    frag_dither = dither;
    frag_semi_transparent = semi_transparent;
    frag_semi_transparency_mode = semi_transparency_mode;
}
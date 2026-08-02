#version 300 es
// flat.vert
precision highp float;

layout(location = 0) in vec2 position;
layout(location = 1) in vec3 colour;
layout(location = 2) in float dither;
layout(location = 3) in float semi_transparent;
layout(location = 4) in float semi_transparency_mode;

uniform float scale; // Maps VRAM-space pixels (1024x512) to the target's pixel space

out vec3 frag_colour;
flat out float frag_dither;
flat out float frag_semi_transparent;
flat out float frag_semi_transparency_mode;

void main() {
    vec2 scaled_pos = position * scale;
    vec2 target_size = vec2(1024.0, 512.0) * scale;
    vec2 ndc = (scaled_pos / target_size) * 2.0 - 1.0;

    gl_Position = vec4(ndc, 0.0, 1.0);
    frag_colour = colour;
    frag_dither = dither;
    frag_semi_transparent = semi_transparent;
    frag_semi_transparency_mode = semi_transparency_mode;
}
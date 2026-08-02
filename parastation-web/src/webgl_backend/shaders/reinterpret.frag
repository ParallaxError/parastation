#version 300 es
// reinterpret.frag: Blit a region from a 15bit BGR555 VRAM texture to a 24bit RGB888 framebuffer, potentially of a
// different size, with scaling and offset
precision highp float;
precision highp int;
precision highp usampler2D;

uniform highp usampler2D accurate_vram;
uniform vec2 src_origin;
uniform vec2 dest_origin;
uniform float inv_scale;

out vec4 out_colour;

void main() {
    vec2 local_frag_coord = gl_FragCoord.xy - dest_origin;
    ivec2 src_pixel = ivec2(src_origin) + ivec2(local_frag_coord * inv_scale);

    uint raw = texelFetch(accurate_vram, src_pixel, 0).r;
    float r = float((raw >>  0u) & 0x1Fu) / 31.0;
    float g = float((raw >>  5u) & 0x1Fu) / 31.0;
    float b = float((raw >> 10u) & 0x1Fu) / 31.0;
    out_colour = vec4(r, g, b, 1.0);
}
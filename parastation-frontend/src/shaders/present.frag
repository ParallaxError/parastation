// PRESENT_FRAG: Simple shader to present the final framebuffer to the screen, sampling from the VRAM texture
#version 330 core
uniform usampler2D vram;
uniform vec2 display_origin;
uniform vec2 display_size;

in vec2 frag_uv;
out vec4 out_colour;

void main() {
    vec2 uv = display_origin + frag_uv * display_size;
    ivec2 coord = ivec2(uv * vec2(1024.0, 512.0));
    uint raw = texelFetch(vram, coord, 0).r;

    // Decode from RGB555 to present an RGBA texture
    float r = float((raw >>  0u) & 0x1Fu) / 31.0;
    float g = float((raw >>  5u) & 0x1Fu) / 31.0;
    float b = float((raw >> 10u) & 0x1Fu) / 31.0;

    out_colour = vec4(r, g, b, 1.0);
}
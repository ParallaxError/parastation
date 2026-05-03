// PRESENT_FRAG: Simple shader to present the final framebuffer to the screen, sampling from the VRAM texture
#version 330 core
uniform sampler2D vram;
uniform vec2 display_origin;  // in VRAM UV space (0..1)
uniform vec2 display_size;    // in VRAM UV space (0..1)

in vec2 frag_uv;
out vec4 out_colour;

void main() {
    vec2 uv = display_origin + frag_uv * display_size;
    out_colour = texture(vram, uv);
}
// Untextured polygon shader, pretty standard
#version 330 core
layout(location = 0) in vec2 position;  // VRAM coords 0..1023, 0..511
layout(location = 1) in vec3 colour;

uniform vec2 drawing_offset;
out vec3 frag_colour;


void main() {
    // vec2 vram_pos = mod(position + drawing_offset, vec2(1024.0, 512.0));
    vec2 vram_pos = position + drawing_offset;

    // Convert VRAM coords to NDC (-1..1)
    vec2 ndc = (vram_pos / vec2(1024.0, 512.0)) * 2.0 - 1.0;
    gl_Position = vec4(ndc, 0.0, 1.0);
    frag_colour = colour / 255.0;
}
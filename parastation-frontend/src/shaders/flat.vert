// Untextured polygon shader, pretty standard
#version 330 core
layout(location = 0) in vec2 position;  // VRAM coords 0..1023, 0..511
layout(location = 1) in vec3 colour;

out vec3 frag_colour;

void main() {
    // Convert VRAM coords to NDC (-1..1)
    vec2 ndc = (position / vec2(1024.0, 512.0)) * 2.0 - 1.0;
    // ndc.y = -ndc.y;  // flip Y since VRAM Y=0 is top but OpenGL Y=0 is bottom
    gl_Position = vec4(ndc, 0.0, 1.0);
    frag_colour = colour / 255.0;
}
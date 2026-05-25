// Textured polygon shader, similar to the flat shader but with extra attributes for texture coordinates and CLUT lookup
#version 330 core
layout(location = 0) in vec2 position;
layout(location = 1) in vec3 colour;
layout(location = 2) in vec2 texcoord;   // UV in texture page
layout(location = 3) in vec2 clut_coord; // CLUT location in VRAM

out vec3 frag_colour;
out vec2 frag_texcoord;
out vec2 frag_clut;

void main() {
    vec2 ndc = (position / vec2(1024.0, 512.0)) * 2.0 - 1.0;
    // ndc.y = -ndc.y;
    gl_Position = vec4(ndc, 0.0, 1.0);
    frag_colour = colour;
    frag_texcoord = texcoord;
    frag_clut = clut_coord;
}
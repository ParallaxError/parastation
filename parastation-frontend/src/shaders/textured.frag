// TEXTURED_FRAG, read from VRAM to index textures
#version 330 core
uniform sampler2D vram;
uniform int tex_depth;   // 0=4bit, 1=8bit, 2=15bit
uniform vec2 texpage;    // texture page origin in VRAM

in vec3 frag_colour;
in vec2 frag_texcoord;
in vec2 frag_clut;
out vec4 out_colour;

void main() {
    // For now — 15bit direct texture
    vec2 uv = (texpage + frag_texcoord) / vec2(1024.0, 512.0);
    vec4 texel = texture(vram, uv);
    out_colour = texel * vec4(frag_colour, 1.0);
}
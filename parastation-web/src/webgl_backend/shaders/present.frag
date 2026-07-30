#version 300 es
precision highp float;

uniform sampler2D source;
in vec2 frag_uv;
out vec4 out_colour;

void main() {
    out_colour = texture(source, frag_uv);
}
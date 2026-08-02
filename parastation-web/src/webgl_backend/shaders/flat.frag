#version 300 es
// flat.frag: For enhanced target, no dithering, RGB888
precision highp float;

in vec3 frag_colour;
out vec4 out_colour;

void main() {
    out_colour = vec4(frag_colour / 255.0, 1.0);
}
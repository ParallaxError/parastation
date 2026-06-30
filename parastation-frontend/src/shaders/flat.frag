// FLAT_FRAG: Simple shader for flat-coloured polygons, converting from 0-255 scale to RGB555 for PS1 output
#version 330 core
in vec3 frag_colour;
out uvec4 out_colour;

void main() {
    float cr = clamp(frag_colour.r, 0.0, 1.0);
    float cg = clamp(frag_colour.g, 0.0, 1.0);
    float cb = clamp(frag_colour.b, 0.0, 1.0);

    uint ir = uint(cr * 31.0 + 0.5);
    uint ig = uint(cg * 31.0 + 0.5);
    uint ib = uint(cb * 31.0 + 0.5);

    uint bgr555 = ir | (ig << 5u) | (ib << 10u);
    out_colour = uvec4(bgr555, 0u, 0u, 0u);
}
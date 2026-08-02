#version 300 es
// reinterpret.vert
precision highp float;

uniform vec2 dest_origin;
uniform vec2 dest_size;

void main() {
    vec2 positions[4] = vec2[](
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0)
    );
    gl_Position = vec4(positions[gl_VertexID], 0.0, 1.0);
}
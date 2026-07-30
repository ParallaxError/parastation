#version 300 es
precision highp float;

uniform vec2 screen_offset;
uniform vec2 screen_size;
uniform vec2 display_origin;
uniform vec2 display_size;

out vec2 frag_uv;

void main() {
    vec2 positions[4] = vec2[](
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(1.0, 1.0)
    );
    vec2 pos = positions[gl_VertexID];

    frag_uv = display_origin + pos * display_size;

    vec2 ndc = screen_offset + pos * screen_size;
    gl_Position = vec4(ndc, 0.0, 1.0);
}
// PRESENT_VERT: single shader for presenting the final framebuffer to the screen, just a full-screen quad with UVs
#version 330 core
uniform vec2 screen_offset;  // NDC offset of display area (-1..1)
uniform vec2 screen_size;    // NDC size of display area (0..2)
out vec2 frag_uv;

void main() {
    vec2 positions[4] = vec2[](
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(1.0, 1.0)
    );
    vec2 uvs[4] = vec2[](
        vec2(0.0, 1.0),
        vec2(1.0, 1.0),
        vec2(0.0, 0.0),
        vec2(1.0, 0.0)
    );
    vec2 ndc = screen_offset + positions[gl_VertexID] * screen_size;
    gl_Position = vec4(ndc, 0.0, 1.0);
    frag_uv = uvs[gl_VertexID];
}
#version 300 es
precision highp float;
precision highp int;
precision highp usampler2D;

uniform sampler2D source;
uniform usampler2D accurate_source;
uniform int colour_depth; // 0 = 15 bit colour (sample regular source), 1 = 24 bit colour (sample accurate source)

in vec2 frag_uv;
out vec4 out_colour;

void main() {
    if (colour_depth == 0) {
        out_colour = texture(source, frag_uv);
        return;
    }

    ivec2 tex_size = textureSize(accurate_source, 0);
    vec2 texel_pos = frag_uv * vec2(tex_size);
    int screen_x = int(texel_pos.x);
    int screen_y = int(texel_pos.y);

    int halfword_base = (screen_x / 2) * 3;

    uint h0 = texelFetch(accurate_source, ivec2(halfword_base + 0, screen_y), 0).r;
    uint h1 = texelFetch(accurate_source, ivec2(halfword_base + 1, screen_y), 0).r;
    uint h2 = texelFetch(accurate_source, ivec2(halfword_base + 2, screen_y), 0).r;

    vec3 colour;
    if ((screen_x & 1) == 0) {
        // Even pixel: B = h0 low byte, G = h0 high byte, R = h1 low byte
        colour = vec3(float(h1 & 0xFFu), float((h0 >> 8u) & 0xFFu), float(h0 & 0xFFu)) / 255.0;
    } else {
        // Odd pixel: B = h1 high byte, G = h2 low byte, R = h2 high byte
        colour = vec3(float((h2 >> 8u) & 0xFFu), float(h2 & 0xFFu), float((h1 >> 8u) & 0xFFu)) / 255.0;
    }

    out_colour = vec4(colour, 1.0);
}
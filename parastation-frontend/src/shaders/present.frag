// PRESENT_FRAG: Simple shader to present the final framebuffer to the screen, sampling from the VRAM texture
#version 330 core
uniform usampler2D vram;
uniform vec2 display_origin;
uniform vec2 display_size;
uniform int display_depth; // 0 = 15bpp RGB555, 1 = 24bpp packed RGB888
in vec2 frag_uv;
out vec4 out_colour;

void main() {
    vec2 uv = display_origin + frag_uv * display_size;

    if (display_depth == 0) {
        // 15bpp, one halfword per pixel BGR555
        ivec2 coord = ivec2(uv * vec2(1024.0, 512.0));
        uint raw = texelFetch(vram, coord, 0).r;

        float r = float((raw >>  0u) & 0x1Fu) / 31.0;
        float g = float((raw >>  5u) & 0x1Fu) / 31.0;
        float b = float((raw >> 10u) & 0x1Fu) / 31.0;
        out_colour = vec4(r, g, b, 1.0);
    } else {
        // 24bpp, every 2 pixels are packed across 3 VRAM halfwords
        int screen_x = int(uv.x * 1024.0 - display_origin.x * 1024.0);
        int screen_y = int(uv.y * 512.0);
        int halfword_base = int(display_origin.x * 1024.0) + (screen_x / 2) * 3;

        uint h0 = texelFetch(vram, ivec2(halfword_base + 0, screen_y), 0).r;
        uint h1 = texelFetch(vram, ivec2(halfword_base + 1, screen_y), 0).r;
        uint h2 = texelFetch(vram, ivec2(halfword_base + 2, screen_y), 0).r;

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
}
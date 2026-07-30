/*
 * @file /parastation-web/src/webgl_backend/render_target.rs
 * @brief
 * A framebuffer and texture that draw calls can render into. Contains no rendering-behavior state, only holds relevant
 * objects for the backend to render into.
 *
 *
 * -----
 */

use glow::{Framebuffer, HasContext, Texture};

pub const VRAM_WIDTH: u32 = 1024;
pub const VRAM_HEIGHT: u32 = 512;

pub struct RenderTarget {
    pub framebuffer: Framebuffer,
    pub texture: Texture,
    pub width: u32,
    pub height: u32,
}

impl RenderTarget {
    pub fn new(
        gl: &glow::Context,
        width: u32,
        height: u32,
        internal_format: i32,
        format: u32,
        ty: u32,
    ) -> Self {
        unsafe {
            let texture = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                internal_format,
                width as i32,
                height as i32,
                0,
                format,
                ty,
                None,
            );
            Self::set_common_tex_params(gl);

            let framebuffer = Self::create_framebuffer(gl, texture);
            Self {
                framebuffer,
                texture,
                width,
                height,
            }
        }
    }

    unsafe fn set_common_tex_params(gl: &glow::Context) {
        unsafe {
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_BASE_LEVEL, 0);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAX_LEVEL, 0);
        }
    }

    unsafe fn create_framebuffer(gl: &glow::Context, texture: Texture) -> Framebuffer {
        unsafe {
            let framebuffer = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );

            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                panic!("RenderTarget FBO incomplete: {status:#x}");
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

            framebuffer
        }
    }
}

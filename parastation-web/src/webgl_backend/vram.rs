/*
 * @file /parastation-web/src/webgl_backend/vram.rs
 * @brief
 * VRAM read/write operations for the WebGL backend. VRAM operations for the accurate framebuffer are simple, but
 * upscaling and reinterpreting are required to blit to the enhanced framebuffer. These operations are done by a shader
 * from the accurate framebuffer to the enhanced framebuffer in this file.
 *
 * -----
 */

// Imports
use glow::HasContext;
use parastation_core::log;

use super::WebGlBackend;
use super::drawing::compile_program;

// VRAM transfer structs
pub struct VramWriteTransfer {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub pixels_left: u32,
    pub pixels: Vec<u16>,
}

// Reinterpret pipeline setup
pub struct ReinterpretUniforms {
    pub accurate_vram: Option<glow::UniformLocation>,
    pub src_origin: Option<glow::UniformLocation>,
    pub inv_scale: Option<glow::UniformLocation>,
    pub dest_origin: Option<glow::UniformLocation>,
    pub dest_size: Option<glow::UniformLocation>,
}

pub struct ReinterpretPipeline {
    pub program: glow::Program,
    pub vao: glow::VertexArray,
    pub uniforms: ReinterpretUniforms,
}

const REINTERPRET_VERT: &str = include_str!("shaders/reinterpret.vert");
const REINTERPRET_FRAG: &str = include_str!("shaders/reinterpret.frag");

pub fn create_reinterpret_pipeline(gl: &glow::Context) -> ReinterpretPipeline {
    unsafe {
        log!("{}", REINTERPRET_FRAG);
        let program = compile_program(gl, REINTERPRET_VERT, REINTERPRET_FRAG);
        let vao = gl.create_vertex_array().unwrap();

        let uniforms = ReinterpretUniforms {
            accurate_vram: gl.get_uniform_location(program, "accurate_vram"),
            src_origin: gl.get_uniform_location(program, "src_origin"),
            inv_scale: gl.get_uniform_location(program, "inv_scale"),
            dest_origin: gl.get_uniform_location(program, "dest_origin"),
            dest_size: gl.get_uniform_location(program, "dest_size"),
        };

        ReinterpretPipeline {
            program,
            vao,
            uniforms,
        }
    }
}

impl WebGlBackend {
    /// Complete a vram transfer by directly copying the accumulated pixels to the accurate framebuffer and blitting
    /// the result to the enhanced framebuffer, handling upscaling and depth reinterpreting
    pub(super) fn complete_vram_write(&mut self, transfer: &mut VramWriteTransfer) {
        let (x, y, w, h) = (transfer.x, transfer.y, transfer.w, transfer.h);

        unsafe {
            // By default OpenGL expects 4-byte alignment for pixel data, but our VRAM is 2 bytes per pixel, so we
            // need to set the unpack alignment to 2
            self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 2);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.accurate_target.texture));
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                x as i32,
                y as i32,
                w as i32,
                h as i32,
                glow::RED_INTEGER,
                glow::UNSIGNED_SHORT,
                glow::PixelUnpackData::Slice(bytemuck::cast_slice(&transfer.pixels)),
            );
            self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        }

        self.reinterpret_blit(x, y, w, h);
    }

    /// Blit a region of the accurate framebuffer to the enhanced framebuffer, handling upscaling and depth reinterpreting
    fn reinterpret_blit(&self, x: u16, y: u16, w: u16, h: u16) {
        let scale = self.enhanced_target.width as f32 / super::render_target::VRAM_WIDTH as f32;

        unsafe {
            // Most of the reinterpreting explanation is in the corresponding fragment shader
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.enhanced_target.framebuffer));
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.viewport(
                (x as f32 * scale) as i32,
                (y as f32 * scale) as i32,
                (w as f32 * scale) as i32,
                (h as f32 * scale) as i32,
            );

            self.gl.use_program(Some(self.reinterpret_pipeline.program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.accurate_target.texture));

            self.gl
                .uniform_1_i32(self.reinterpret_pipeline.uniforms.accurate_vram.as_ref(), 0);
            self.gl.uniform_2_f32(
                self.reinterpret_pipeline.uniforms.src_origin.as_ref(),
                x as f32,
                y as f32,
            );
            self.gl.uniform_1_f32(
                self.reinterpret_pipeline.uniforms.inv_scale.as_ref(),
                1.0 / scale,
            );
            self.gl.uniform_2_f32(
                self.reinterpret_pipeline.uniforms.dest_origin.as_ref(),
                x as f32 * scale,
                y as f32 * scale,
            );
            self.gl.uniform_2_f32(
                self.reinterpret_pipeline.uniforms.dest_size.as_ref(),
                w as f32 * scale,
                h as f32 * scale,
            );

            self.gl
                .bind_vertex_array(Some(self.reinterpret_pipeline.vao));
            self.gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
    }
}

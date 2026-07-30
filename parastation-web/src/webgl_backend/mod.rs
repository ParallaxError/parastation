/*
 * @file /parastation-web/src/webgl_backend/mod.rs
 * @brief
 * Web GL implementation for the ParaStation GPUBackend trait.
 * Maps pretty similarly to parastation-frontend/opengl_backend.rs since WebGL is pretty simialr to OpenGL but
 * VRAM operations are handled differently, and some unsupported features must be replaced with workarounds.
 *
 * -----
 */

mod drawing;
mod render_target;

// Imports
use parastation_core::gpu::backend::GpuBackend;
use parastation_core::gpu::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext};

use render_target::{RenderTarget, VRAM_HEIGHT, VRAM_WIDTH};

/// Acquire a WebGL2 context from a canvas element and wrap it into a glow::Context
pub fn create_gl_context(canvas: &HtmlCanvasElement) -> glow::Context {
    let webgl2_context = canvas
        .get_context("webgl2")
        .expect("Failed to query webgl2 context")
        .expect("WebGL2 not supported by this browser")
        .dyn_into::<WebGl2RenderingContext>()
        .expect("Failed to cast to WebGl2RenderingContext");

    glow::Context::from_webgl2_context(webgl2_context)
}

/// Cached uniforms for the present shader program to avoid calling glGetUniformLocation every frame call
pub struct PresentUniforms {
    pub source: Option<glow::UniformLocation>,
    pub display_origin: Option<glow::UniformLocation>,
    pub display_size: Option<glow::UniformLocation>,
    pub screen_offset: Option<glow::UniformLocation>,
    pub screen_size: Option<glow::UniformLocation>,
}

/// WebGL backend implementation of the GPUBackend trait. Has some enhancements over the OpenGL backend, with upscaled
/// and 24bit colour rendering
pub struct WebGlBackend {
    gl: glow::Context,

    accurate_target: RenderTarget,
    enhanced_target: RenderTarget,

    present_program: glow::Program,
    present_vao: glow::VertexArray,
    present_uniforms: PresentUniforms,

    canvas_width: u32,
    canvas_height: u32,
}

impl WebGlBackend {
    pub fn new(gl: glow::Context, canvas_width: u32, canvas_height: u32) -> Self {
        let accurate_target = RenderTarget::new(
            &gl,
            VRAM_WIDTH,
            VRAM_HEIGHT,
            glow::R16UI as i32,
            glow::RED_INTEGER,
            glow::UNSIGNED_SHORT,
        );
        let enhanced_target = RenderTarget::new(
            &gl,
            VRAM_WIDTH,
            VRAM_HEIGHT,
            glow::RGBA8 as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
        ); // 1x for now

        let (present_program, present_vao, present_uniforms) =
            drawing::create_present_pipeline(&gl);

        Self {
            gl,
            accurate_target,
            enhanced_target,
            present_program,
            present_vao,
            present_uniforms,
            canvas_width,
            canvas_height,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.canvas_width = width;
        self.canvas_height = height;
    }
}

impl GpuBackend for WebGlBackend {
    fn draw_polygon(&mut self, polygon: &Polygon, params: &DrawParams) {}

    fn draw_line(&mut self, line: &Line, params: &DrawParams) {}

    fn draw_rect(&mut self, rect: &Rect, params: &DrawParams) {}

    fn fill_rect(&mut self, pos: Vertex, w: u16, h: u16, colour: Colour) {}

    fn clear_cache(&mut self) {}

    fn copy_rect(
        &mut self,
        src_x: u16,
        src_y: u16,
        dst_x: u16,
        dst_y: u16,
        w: u16,
        h: u16,
        mask: &Mask,
    ) {
    }

    fn vram_read_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16) {}

    fn vram_read(&mut self) -> Option<u32> {
        None
    }

    fn vram_write_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16, mask: &Mask) {}

    fn vram_write(&mut self, word: u32) {}

    fn present(&mut self, output: &DisplayOutput) {
        drawing::present(
            &self.gl,
            self.present_program,
            self.present_vao,
            &self.present_uniforms,
            &self.enhanced_target,
            self.canvas_width,
            self.canvas_height,
            output,
        );
    }
}

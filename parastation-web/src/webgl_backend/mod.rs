/*
 * @file /parastation-web/src/webgl_backend/mod.rs
 * @brief
 * Web GL implementation for the ParaStation GPUBackend trait.
 * Maps pretty similarly to parastation-frontend/opengl_backend.rs since WebGL is pretty simialr to OpenGL but
 * VRAM operations are handled differently, and some unsupported features must be replaced with workarounds.
 *
 * -----
 */

mod batch;
mod drawing;
mod render_target;
pub mod shared_gpu_handle;

// Imports
use glow::HasContext;
use parastation_core::gpu::backend::GpuBackend;
use parastation_core::gpu::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext};

use batch::FlatBatch;
use drawing::*;
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

    canvas_width: u32,
    canvas_height: u32,

    present_pipeline: PresentPipeline,

    flat_batch: FlatBatch,
    flat_pipeline: FlatPipeline,
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
            VRAM_WIDTH * 2,
            VRAM_HEIGHT * 2,
            glow::RGBA8 as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
        );

        let present_pipeline = create_present_pipeline(&gl);

        let flat_batch = FlatBatch::new();
        let flat_pipeline = create_flat_pipeline(&gl);

        Self {
            gl,
            accurate_target,
            enhanced_target,
            canvas_width,
            canvas_height,
            present_pipeline,
            flat_batch,
            flat_pipeline,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.canvas_width = width;
        self.canvas_height = height;
    }

    /// Flush all drawing batches to both targets, preparing for a VRAM read or present call
    fn flush(&mut self) {
        self.flush_flat();
        self.flush_textured();
    }
}

impl GpuBackend for WebGlBackend {
    fn draw_polygon(&mut self, polygon: &Polygon, params: &DrawParams) {
        let dither = params.draw_mode.dither;
        let semi_transparency_mode = params.draw_mode.semi_transparency;
        match polygon {
            Polygon::Monochrome {
                colour,
                vertices,
                semi_transparent,
            } => {
                vertices.triangles(|v0, v1, v2| {
                    let verts = [
                        FlatGlVertex::new(
                            v0.vertex.x,
                            v0.vertex.y,
                            *colour,
                            dither,
                            *semi_transparent,
                            semi_transparency_mode,
                        ),
                        FlatGlVertex::new(
                            v1.vertex.x,
                            v1.vertex.y,
                            *colour,
                            dither,
                            *semi_transparent,
                            semi_transparency_mode,
                        ),
                        FlatGlVertex::new(
                            v2.vertex.x,
                            v2.vertex.y,
                            *colour,
                            dither,
                            *semi_transparent,
                            semi_transparency_mode,
                        ),
                    ];
                    self.queue_flat(&verts, &params.drawing_area, &params.drawing_offset);
                });
            }
            Polygon::Shaded {
                vertices,
                semi_transparent,
            } => {
                vertices.triangles(|v0, v1, v2| {
                    let verts = [
                        FlatGlVertex::new(
                            v0.vertex.x,
                            v0.vertex.y,
                            v0.colour,
                            dither,
                            *semi_transparent,
                            semi_transparency_mode,
                        ),
                        FlatGlVertex::new(
                            v1.vertex.x,
                            v1.vertex.y,
                            v1.colour,
                            dither,
                            *semi_transparent,
                            semi_transparency_mode,
                        ),
                        FlatGlVertex::new(
                            v2.vertex.x,
                            v2.vertex.y,
                            v2.colour,
                            dither,
                            *semi_transparent,
                            semi_transparency_mode,
                        ),
                    ];
                    self.queue_flat(&verts, &params.drawing_area, &params.drawing_offset);
                });
            }
            _ => {}
        }
    }

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
        // Flush any pending batches before presenting
        self.flush();

        drawing::present(
            &self.gl,
            &self.present_pipeline,
            &self.enhanced_target,
            self.canvas_width,
            self.canvas_height,
            output,
        );
    }
}

// Debug methods
impl WebGlBackend {
    pub fn dump_accurate_target(&self) -> (u32, u32, Vec<u8>) {
        let width = self.accurate_target.width;
        let height = self.accurate_target.height;

        let mut raw = vec![0u16; (width * height) as usize];
        unsafe {
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.accurate_target.framebuffer));
            self.gl.read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                glow::RED_INTEGER,
                glow::UNSIGNED_SHORT,
                glow::PixelPackData::Slice(bytemuck::cast_slice_mut(&mut raw)),
            );
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }

        // Decode RGB555 -> RGBA8 for viewing
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for pixel in raw {
            let r = ((pixel & 0x1F) as u32 * 255 / 31) as u8;
            let g = (((pixel >> 5) & 0x1F) as u32 * 255 / 31) as u8;
            let b = (((pixel >> 10) & 0x1F) as u32 * 255 / 31) as u8;
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(255);
        }

        (width, height, rgba)
    }

    pub fn dump_enhanced_target(&self) -> (u32, u32, Vec<u8>) {
        let width = self.enhanced_target.width;
        let height = self.enhanced_target.height;

        let mut rgba = vec![0u8; (width * height * 4) as usize];
        unsafe {
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.enhanced_target.framebuffer));
            self.gl.read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(&mut rgba),
            );
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }

        (width, height, rgba)
    }
}

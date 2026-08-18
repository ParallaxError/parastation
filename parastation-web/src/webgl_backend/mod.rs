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
mod vram;

// Imports
use glow::HasContext;
use parastation_core::gpu::backend::GpuBackend;
use parastation_core::{elog, gpu::*};
use wasm_bindgen::JsCast;
use web_sys::{OffscreenCanvas, WebGl2RenderingContext};

use batch::{FlatBatch, TexturedBatch};
use drawing::*;
use render_target::{RenderTarget, VRAM_HEIGHT, VRAM_WIDTH};
use vram::*;

/// Acquire a WebGL2 context from an OffscreenCanvas and wrap it into a glow::Context
pub fn create_gl_context(canvas: &OffscreenCanvas) -> glow::Context {
    let webgl2_context = canvas
        .get_context("webgl2")
        .expect("Failed to query webgl2 context")
        .expect("WebGL2 not supported by this browser")
        .dyn_into::<WebGl2RenderingContext>()
        .expect("Failed to cast to WebGl2RenderingContext");

    glow::Context::from_webgl2_context(webgl2_context)
}

/// WebGL backend implementation of the GPUBackend trait. Has some enhancements over the OpenGL backend, with upscaled
/// and 24bit colour rendering
pub struct WebGlBackend {
    gl: glow::Context,

    accurate_target: RenderTarget,
    accurate_sample: RenderTarget, // Synced copy used for self referential shader reads (transparency, textures)
    enhanced_target: RenderTarget,
    enhanced_sample: RenderTarget, // Synced copy just for enhanced semi-transparency rendering

    canvas_width: u32,
    canvas_height: u32,

    // VRAM transfers
    vram_write_transfer: Option<VramWriteTransfer>,
    vram_read_transfer: Option<VramReadTransfer>,

    present_pipeline: PresentPipeline,
    reinterpret_pipeline: ReinterpretPipeline,

    flat_batch: FlatBatch,
    flat_pipeline: FlatPipeline,

    textured_batch: TexturedBatch,
    textured_pipeline: TexturedPipeline,
}

impl WebGlBackend {
    pub fn new(gl: glow::Context, canvas_width: u32, canvas_height: u32, scale: u32) -> Self {
        let accurate_target = RenderTarget::new(
            &gl,
            VRAM_WIDTH,
            VRAM_HEIGHT,
            glow::R16UI as i32,
            glow::RED_INTEGER,
            glow::UNSIGNED_SHORT,
        );
        let accurate_sample = RenderTarget::new(
            &gl,
            VRAM_WIDTH,
            VRAM_HEIGHT,
            glow::R16UI as i32,
            glow::RED_INTEGER,
            glow::UNSIGNED_SHORT,
        );

        let enhanced_target = RenderTarget::new(
            &gl,
            VRAM_WIDTH * scale,
            VRAM_HEIGHT * scale,
            glow::RGBA8 as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
        );

        let enhanced_sample = RenderTarget::new(
            &gl,
            VRAM_WIDTH * scale,
            VRAM_HEIGHT * scale,
            glow::RGBA8 as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
        );

        let present_pipeline = create_present_pipeline(&gl);
        let reinterpret_pipeline = create_reinterpret_pipeline(&gl);

        let flat_batch = FlatBatch::new();
        let flat_pipeline = create_flat_pipeline(&gl);

        let textured_batch = TexturedBatch::new();
        let textured_pipeline = create_textured_pipeline(&gl);

        Self {
            gl,
            accurate_target,
            accurate_sample,
            enhanced_target,
            enhanced_sample,
            canvas_width,
            canvas_height,
            present_pipeline,
            reinterpret_pipeline,
            vram_write_transfer: None,
            vram_read_transfer: None,
            flat_batch,
            flat_pipeline,
            textured_batch,
            textured_pipeline,
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

    fn sync_samples(&mut self) {
        RenderTarget::blit_full(&self.gl, &self.accurate_target, &self.accurate_sample);
        RenderTarget::blit_full(&self.gl, &self.enhanced_target, &self.enhanced_sample);
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
                let make_vert = |v: &FlatVertex| {
                    FlatGlVertex::new(
                        v.vertex.x,
                        v.vertex.y,
                        *colour,
                        dither,
                        *semi_transparent,
                        semi_transparency_mode,
                    )
                };

                vertices.triangles(|v0, v1, v2| {
                    let verts = [make_vert(&v0), make_vert(&v1), make_vert(&v2)];
                    self.queue_flat(
                        &verts,
                        &params.drawing_area,
                        &params.drawing_offset,
                        glow::TRIANGLES,
                    );
                });
            }
            Polygon::Shaded {
                vertices,
                semi_transparent,
            } => {
                let make_vert = |v: &ShadedVertex| {
                    FlatGlVertex::new(
                        v.vertex.x,
                        v.vertex.y,
                        v.colour,
                        dither,
                        *semi_transparent,
                        semi_transparency_mode,
                    )
                };

                vertices.triangles(|v0, v1, v2| {
                    let verts = [make_vert(&v0), make_vert(&v1), make_vert(&v2)];
                    self.queue_flat(
                        &verts,
                        &params.drawing_area,
                        &params.drawing_offset,
                        glow::TRIANGLES,
                    );
                });
            }
            Polygon::Textured {
                colour,
                texture_params,
                vertices,
                semi_transparent,
            } => {
                let tex_x = (texture_params.tex_page.x as u16) * 64;
                let tex_y: u16 = if texture_params.tex_page.y { 256 } else { 0 };
                let tex_window = &params.texture_window;

                let make_vert = |v: &TexturedVertex| {
                    TexturedGlVertex::new(
                        v.vertex.x,
                        v.vertex.y,
                        *colour,
                        v.texcoord.u as f32,
                        v.texcoord.v as f32,
                        texture_params.clut.x as u16 * 16,
                        texture_params.clut.y as u16,
                        tex_x,
                        tex_y,
                        texture_params.tex_page.colour_depth as u8,
                        tex_window.texture_window_mask_x,
                        tex_window.texture_window_mask_y,
                        tex_window.texture_window_offset_x as i8,
                        tex_window.texture_window_offset_y as i8,
                        texture_params.raw_texture,
                        dither,
                        *semi_transparent,
                        texture_params.tex_page.semi_transparency,
                    )
                };

                vertices.triangles(|v0, v1, v2| {
                    let verts = [make_vert(&v0), make_vert(&v1), make_vert(&v2)];
                    self.queue_textured(&verts, &params.drawing_area, &params.drawing_offset);
                });
            }
            Polygon::ShadedTextured {
                texture_params,
                vertices,
                semi_transparent,
            } => {
                let tex_x = (texture_params.tex_page.x as u16) * 64;
                let tex_y: u16 = if texture_params.tex_page.y { 256 } else { 0 };
                let tex_window = &params.texture_window;

                let make_vert = |v: &ShadedTexturedVertex| {
                    TexturedGlVertex::new(
                        v.vertex.x,
                        v.vertex.y,
                        v.colour,
                        v.texcoord.u as f32,
                        v.texcoord.v as f32,
                        texture_params.clut.x as u16 * 16,
                        texture_params.clut.y as u16,
                        tex_x,
                        tex_y,
                        texture_params.tex_page.colour_depth as u8,
                        tex_window.texture_window_mask_x,
                        tex_window.texture_window_mask_y,
                        tex_window.texture_window_offset_x as i8,
                        tex_window.texture_window_offset_y as i8,
                        texture_params.raw_texture,
                        dither,
                        *semi_transparent,
                        texture_params.tex_page.semi_transparency,
                    )
                };

                vertices.triangles(|v0, v1, v2| {
                    let verts = [make_vert(&v0), make_vert(&v1), make_vert(&v2)];
                    self.queue_textured(&verts, &params.drawing_area, &params.drawing_offset);
                });
            }
            _ => {}
        }
    }

    fn draw_line(&mut self, line: &Line, params: &DrawParams) {
        let dither = params.draw_mode.dither;
        let semi_transparency_mode = params.draw_mode.semi_transparency;

        match line {
            Line::Monochrome {
                colour,
                vertices,
                semi_transparent,
            } => {
                let verts: Vec<FlatGlVertex> = vertices
                    .iter()
                    .map(|v| {
                        FlatGlVertex::new(
                            v.vertex.x,
                            v.vertex.y,
                            *colour,
                            dither,
                            *semi_transparent,
                            semi_transparency_mode,
                        )
                    })
                    .collect();

                let mode = if verts.len() > 2 {
                    glow::LINE_STRIP
                } else {
                    glow::LINES
                };
                self.queue_flat(&verts, &params.drawing_area, &params.drawing_offset, mode);
            }
            Line::Coloured {
                vertices,
                semi_transparent,
            } => {
                let verts: Vec<FlatGlVertex> = vertices
                    .iter()
                    .map(|v| {
                        FlatGlVertex::new(
                            v.vertex.x,
                            v.vertex.y,
                            v.colour,
                            dither,
                            *semi_transparent,
                            semi_transparency_mode,
                        )
                    })
                    .collect();

                let mode = if verts.len() > 2 {
                    glow::LINE_STRIP
                } else {
                    glow::LINES
                };
                self.queue_flat(&verts, &params.drawing_area, &params.drawing_offset, mode);
            }
        }
    }

    fn draw_rect(&mut self, rect: &Rect, params: &DrawParams) {
        let dither = params.draw_mode.dither;

        match rect {
            Rect::Monochrome {
                colour,
                pos,
                size,
                semi_transparent,
            } => {
                let (w, h) = match size {
                    RectSize::Variable { w, h } => (*w as i16, *h as i16),
                    RectSize::Fixed1x1 => (1, 1),
                    RectSize::Fixed8x8 => (8, 8),
                    RectSize::Fixed16x16 => (16, 16),
                };
                let x0 = pos.x;
                let y0 = pos.y;
                let x1 = x0 + w;
                let y1 = y0 + h;

                let semi_transparency_mode = params.draw_mode.semi_transparency;
                let flat = |x, y| {
                    FlatGlVertex::new(
                        x,
                        y,
                        *colour,
                        dither,
                        *semi_transparent,
                        semi_transparency_mode,
                    )
                };

                // PS1 splits quads as (v0,v1,v2) and (v1,v2,v3)
                let verts = [
                    flat(x0, y0),
                    flat(x1, y0),
                    flat(x0, y1),
                    flat(x1, y0),
                    flat(x0, y1),
                    flat(x1, y1),
                ];
                self.queue_flat(
                    &verts,
                    &params.drawing_area,
                    &params.drawing_offset,
                    glow::TRIANGLES,
                );
            }

            Rect::Textured {
                colour,
                pos,
                size,
                texcoord,
                semi_transparent,
                clut,
                raw,
            } => {
                let (w, h) = match size {
                    RectSize::Variable { w, h } => (*w as i16, *h as i16),
                    RectSize::Fixed1x1 => (1, 1),
                    RectSize::Fixed8x8 => (8, 8),
                    RectSize::Fixed16x16 => (16, 16),
                };

                let mode = &params.draw_mode;
                let tex_window = &params.texture_window;

                let tex_x = (mode.texture_base_x as u16) * 64;
                let tex_y: u16 = if mode.texture_base_y { 256 } else { 0 };

                let u0 = texcoord.u as f32;
                let v0 = texcoord.v as f32;
                let u1 = u0 + w as f32;
                let v1 = v0 + h as f32;

                let (u0, u1) = if mode.textured_rectangle_flip_x {
                    (u1, u0)
                } else {
                    (u0, u1)
                };
                let (v0, v1) = if mode.textured_rectangle_flip_y {
                    (v1, v0)
                } else {
                    (v0, v1)
                };

                let x0 = pos.x;
                let y0 = pos.y;
                let x1 = x0 + w;
                let y1 = y0 + h;

                let make_vert = |x: i16, y: i16, u: f32, v: f32| {
                    TexturedGlVertex::new(
                        x,
                        y,
                        *colour,
                        u,
                        v,
                        clut.x as u16 * 16,
                        clut.y as u16,
                        tex_x,
                        tex_y,
                        mode.texture_page_colours as u8,
                        tex_window.texture_window_mask_x,
                        tex_window.texture_window_mask_y,
                        tex_window.texture_window_offset_x as i8,
                        tex_window.texture_window_offset_y as i8,
                        *raw,
                        dither,
                        *semi_transparent,
                        mode.semi_transparency,
                    )
                };

                let verts = [
                    make_vert(x0, y0, u0, v0),
                    make_vert(x1, y0, u1, v0),
                    make_vert(x0, y1, u0, v1),
                    make_vert(x1, y0, u1, v0),
                    make_vert(x0, y1, u0, v1),
                    make_vert(x1, y1, u1, v1),
                ];
                self.queue_textured(&verts, &params.drawing_area, &params.drawing_offset);
            }
        }
    }

    fn fill_rect(&mut self, pos: Vertex, w: u16, h: u16, colour: Colour) {
        let x0 = pos.x;
        let y0 = pos.y;
        let x1 = x0 + w as i16;
        let y1 = y0 + h as i16;

        // No dither, no semi transparency, no semi transparency mode for fill_rect
        let flat = |x, y| FlatGlVertex::new(x, y, colour, false, false, 0);

        let drawing_area = DrawingArea {
            x1: 0,
            y1: 0,
            x2: 1024,
            y2: 512,
        };
        let drawing_offset = DrawingOffset { x: 0, y: 0 };

        let verts = [
            flat(x0, y0),
            flat(x1, y0),
            flat(x0, y1),
            flat(x1, y0),
            flat(x0, y1),
            flat(x1, y1),
        ];
        self.queue_flat(&verts, &drawing_area, &drawing_offset, glow::TRIANGLES);
    }

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
        // TODO handle mask
        if mask.check_mask_before_draw || mask.set_mask_while_drawing {
            elog!("copy_rect: mask check not implemented");
        }

        // Can't blit from one texture to the same in WebGL, so we need to use the synced sample copies
        self.sync_samples();

        unsafe {
            self.gl.bind_framebuffer(
                glow::READ_FRAMEBUFFER,
                Some(self.accurate_sample.framebuffer),
            );
            self.gl.bind_framebuffer(
                glow::DRAW_FRAMEBUFFER,
                Some(self.accurate_target.framebuffer),
            );
            self.gl.blit_framebuffer(
                src_x as i32,
                src_y as i32,
                (src_x + w) as i32,
                (src_y + h) as i32,
                dst_x as i32,
                dst_y as i32,
                (dst_x + w) as i32,
                (dst_y + h) as i32,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            self.gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);

            self.gl.bind_framebuffer(
                glow::READ_FRAMEBUFFER,
                Some(self.enhanced_sample.framebuffer),
            );
            self.gl.bind_framebuffer(
                glow::DRAW_FRAMEBUFFER,
                Some(self.enhanced_target.framebuffer),
            );
            self.gl.blit_framebuffer(
                src_x as i32,
                src_y as i32,
                (src_x + w) as i32,
                (src_y + h) as i32,
                dst_x as i32,
                dst_y as i32,
                (dst_x + w) as i32,
                (dst_y + h) as i32,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            self.gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
        }
    }

    fn vram_read_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16) {
        // Flush any pending batches before reading VRAM
        self.flush();

        // Create a VRAM read transfer and populate it with the current pixels in the accurate framebuffer
        let transfer = self.create_vram_read_transfer(vram_x, vram_y, w, h);
        self.vram_read_transfer = Some(transfer);
    }

    fn vram_read(&mut self) -> Option<u32> {
        let Some(transfer) = &mut self.vram_read_transfer else {
            return None;
        };

        if transfer.cursor < transfer.pixels.len() as u32 {
            // Read two pixels and return them as a single u32 word
            let pixel1 = transfer.pixels[transfer.cursor as usize] as u32;
            transfer.cursor += 1;
            let pixel2 = if transfer.cursor < transfer.pixels.len() as u32 {
                let p = transfer.pixels[transfer.cursor as usize] as u32;
                transfer.cursor += 1;
                p
            } else {
                0
            };

            Some((pixel2 << 16) | pixel1)
        } else {
            self.vram_read_transfer = None;
            None
        }
    }

    fn vram_write_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16, _mask: &Mask) {
        self.vram_write_transfer = Some(VramWriteTransfer {
            x: vram_x,
            y: vram_y,
            w,
            h,
            pixels_left: w as u32 * h as u32,
            pixels: Vec::with_capacity(w as usize * h as usize),
        })
    }

    fn vram_write(&mut self, word: u32) {
        let Some(transfer) = &mut self.vram_write_transfer else {
            return;
        };

        for pixel in [word as u16, (word >> 16) as u16] {
            if transfer.pixels_left == 0 {
                break;
            }
            transfer.pixels.push(pixel);
            transfer.pixels_left -= 1;
        }

        if transfer.pixels_left == 0 {
            let mut completed = self.vram_write_transfer.take().unwrap();
            self.complete_vram_write(&mut completed);
        }
    }

    fn present(&mut self, output: &DisplayOutput) {
        // Flush any pending batches before presenting
        self.flush();

        drawing::present(
            &self.gl,
            &self.present_pipeline,
            &self.enhanced_target,
            &self.accurate_target,
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

    pub fn dump_accurate_sample(&self) -> (u32, u32, Vec<u8>) {
        let width = self.accurate_sample.width;
        let height = self.accurate_sample.height;

        let mut raw = vec![0u16; (width * height) as usize];
        unsafe {
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.accurate_sample.framebuffer));
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

    pub fn dump_enhanced_sample(&self) -> (u32, u32, Vec<u8>) {
        let width = self.enhanced_sample.width;
        let height = self.enhanced_sample.height;

        let mut rgba = vec![0u8; (width * height * 4) as usize];
        unsafe {
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.enhanced_sample.framebuffer));
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

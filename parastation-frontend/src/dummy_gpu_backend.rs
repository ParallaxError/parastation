/*
 * @file /parastation-frontend/src/dummy_gpu_backend.rs
 * @brief
 * GPU backend trait stub, temporary: probably will delete soon.
 *
 * -----
 */

// Imports
use parastation_core::gpu::backend::GpuBackend;
use parastation_core::gpu::{Colour, DrawParams, Line, Mask, Polygon, Rect, Vertex};

pub struct DummyGpuBackend;

impl DummyGpuBackend {
    pub fn new() -> Self {
        Self
    }
}

impl GpuBackend for DummyGpuBackend {
    fn draw_polygon(&mut self, polygon: &Polygon, params: &DrawParams) {
        println!("draw_polygon: {:#?}\nparams: {:#?}", polygon, params);
    }

    fn draw_line(&mut self, line: &Line, params: &DrawParams) {
        println!("draw_line: {:#?}\nparams: {:#?}", line, params);
    }

    fn draw_rect(&mut self, rect: &Rect, params: &DrawParams) {
        println!("draw_rect: {:#?}\nparams: {:#?}", rect, params);
    }

    fn fill_rect(&mut self, pos: Vertex, w: u16, h: u16, colour: Colour) {
        println!(
            "fill_rect: {:#?}, w: {}, h: {}, colour: {:#?}",
            pos, w, h, colour
        );
    }

    fn clear_cache(&mut self) {
        println!("clear_cache");
    }

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
        println!(
            "copy_rect: src_x: {}, src_y: {}, dst_x: {}, dst_y: {}, w: {}, h: {}, mask: {:#?}",
            src_x, src_y, dst_x, dst_y, w, h, mask
        );
    }

    fn vram_read_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16) {
        println!("vram_read_begin: {}, {}, {}, {}", vram_x, vram_y, w, h);
    }

    fn vram_read(&mut self) -> Option<u32> {
        println!("vram_read");
        None
    }

    fn vram_write_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16, mask: &Mask) {
        println!(
            "vram_write_begin: {}, {}, {}, {}, {:#?}",
            vram_x, vram_y, w, h, mask
        );
    }

    fn vram_write(&mut self, word: u32) {
        println!("vram_write: {:#?}", word);
    }

    fn present(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16) {
        println!("present: {}, {}", vram_x, vram_y);
    }
}

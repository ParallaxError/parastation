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
	fn draw_polygon(&mut self, _polygon: &Polygon, _params: &DrawParams) {
		println!("draw_polygon");
	}

	fn draw_line(&mut self, _line: &Line, _params: &DrawParams) {
		println!("draw_line");
	}

	fn draw_rect(&mut self, _rect: &Rect, _params: &DrawParams) {
		println!("draw_rect");
	}

	fn fill_rect(&mut self, _pos: Vertex, _w: u16, _h: u16, _colour: Colour) {
		println!("fill_rect");
	}

	fn copy_rect(&mut self, _src_x: u16, _src_y: u16, _dst_x: u16, _dst_y: u16, _w: u16, _h: u16, _mask: &Mask) {
		println!("copy_rect");
	}

	fn vram_read_begin(&mut self, _vram_x: u16, _vram_y: u16, _w: u16, _h: u16) {
		println!("vram_read_begin");
	}

	fn vram_read(&mut self) -> Option<u32> {
		println!("vram_read");
		None
	}

	fn vram_write_begin(&mut self, _vram_x: u16, _vram_y: u16, _w: u16, _h: u16, _mask: &Mask) {
		println!("vram_write_begin");
	}

	fn vram_write(&mut self, _word: u32) {
		println!("vram_write");
	}

	fn present(&mut self, _vram_x: u16, _vram_y: u16, _w: u16, _h: u16) {
		println!("present");
	}
}
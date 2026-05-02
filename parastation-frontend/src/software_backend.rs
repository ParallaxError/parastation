/*
 * @file /parastation-frontend/src/software_backend.rs
 * @brief
 * Software GPU backend implementation, using the pixels crate to render graphics.
 * 
 * -----
 */

// Imports
use pixels::{Pixels, SurfaceTexture};
use winit::event::Event;
use winit::window::Window;
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;
use parastation_core::gpu::backend::GpuBackend;
use parastation_core::gpu::{Colour, DrawParams, Line, Mask, Polygon, Rect, Vertex, RectSize};

pub struct SoftwareGpuBackend {
	pixels: Pixels,
	window: Window,
	vram: Box<[u16]>, // 1024x512 pixels, 16 bits per pixel
}

impl SoftwareGpuBackend {
    pub fn new(event_loop: &EventLoop<()>) -> Self {
		let scale = 3u32;
		let display_w = 320u32;
		let display_h = 240u32;

		let window = WindowBuilder::new()
			.with_title("parastation")
			.with_inner_size(winit::dpi::LogicalSize::new(
				display_w * scale, 
				display_h * scale
			))
			.build(event_loop)
			.unwrap();

		let surface_texture = SurfaceTexture::new(
			display_w * scale,
			display_h * scale,
			&window
		);

		let pixels = Pixels::new(display_w, display_h, surface_texture).unwrap();
        
		Self {
			pixels,
			window,
	        vram: vec![0u16; 1024 * 512].into_boxed_slice(),
        }
    }

	fn set_pixel(&mut self, x: u16, y: u16, colour: Colour) {
		if x < 1024 && y < 512 {
			let index = (y as usize * 1024 + x as usize) as usize;
			self.vram[index] = colour.to_u16();
		}
	}

	fn set_pixel_masked(&mut self, x: u16, y: u16, colour: Colour, mask: &Mask) {
		// Check mask bit before writing
		if x >= 1024 || y >= 512 {
			return; // Out of bounds
		}

		let index = y as usize * 1024 + x as usize;
		if mask.check_mask_before_draw && (self.vram[index] & 0x8000) != 0 {
			return; // Mask bit set, skip drawing
		}

		let pixel = if mask.set_mask_while_drawing {
			colour.to_u16() | 0x8000 // Set mask bit in VRAM
		} else {
			colour.to_u16()
		};

		self.vram[index] = pixel;
	}
}

impl GpuBackend for SoftwareGpuBackend {
	fn draw_polygon(&mut self, polygon: &Polygon, params: &DrawParams) {
		println!("draw_polygon: {:#?}\nparams: {:#?}", polygon, params);
	}

	fn draw_line(&mut self, line: &Line, params: &DrawParams) {
		println!("draw_line: {:#?}\nparams: {:#?}", line, params);
	}

	fn draw_rect(&mut self, rect: &Rect, params: &DrawParams) {
		match rect {
			Rect::Monochrome { colour, pos, size, semi_transparent } => {
				let (w, h) = match size {
					RectSize::Variable { w, h } => (*w, *h),
					RectSize::Fixed1x1 => (1, 1),
					RectSize::Fixed8x8 => (8, 8),
					RectSize::Fixed16x16 => (16, 16),
				};

				// Apply drawing offset
				let x0 = pos.x + params.drawing_offset.x;
				let y0 = pos.y + params.drawing_offset.y;

				for dy in 0..h as i16 {
					for dx in 0..w	as i16 {
						let px = x0 + dx;
						let py = y0 + dy;

						// Clip to drawing area
						if px < params.drawing_area.x1 as i16
							|| px >= params.drawing_area.x2 as i16
							|| py < params.drawing_area.y1 as i16
							|| py >= params.drawing_area.y2 as i16
						{
							continue;
						}

						self.set_pixel_masked(px as u16, py as u16, *colour, &params.mask);
					}
				}
			}

			_ => { println!("draw_rect: {:#?}\nparams: {:#?}", rect, params); }
		}
	}

	fn fill_rect(&mut self, pos: Vertex, w: u16, h: u16, colour: Colour) {
		println!("fill_rect: {:#?}, w: {}, h: {}, colour: {:#?}", pos, w, h, colour);
	}

	fn copy_rect(&mut self, src_x: u16, src_y: u16, dst_x: u16, dst_y: u16, w: u16, h: u16, mask: &Mask) {
		println!("copy_rect: src_x: {}, src_y: {}, dst_x: {}, dst_y: {}, w: {}, h: {}, mask: {:#?}", src_x, src_y, dst_x, dst_y, w, h, mask);
	}

	fn vram_read_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16) {
		println!("vram_read_begin: {}, {}, {}, {}", vram_x, vram_y, w, h);
	}

	fn vram_read(&mut self) -> Option<u32> {
		println!("vram_read");
		None
	}

	fn vram_write_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16, mask: &Mask) {
		println!("vram_write_begin: {}, {}, {}, {}, {:#?}", vram_x, vram_y, w, h, mask);
	}

	fn vram_write(&mut self, word: u32) {
		println!("vram_write: {:#?}", word);
	}
	
	fn present(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16) {
		let frame_w = 320usize;
		let frame_h = 240usize;
		let frame = self.pixels.frame_mut();
		
		for dy in 0..frame_h {
			for dx in 0..frame_w {
				let vx = vram_x + dx as u16;
				let vy = vram_y + dy as u16;
				let pixel = self.vram[vy as usize * 1024 + vx as usize];
				let r = ((pixel & 0x1F) << 3) as u8;
				let g = (((pixel >> 5) & 0x1F) << 3) as u8;
				let b = (((pixel >> 10) & 0x1F) << 3) as u8;
				let i = (dy * frame_w + dx) * 4;
				frame[i]     = r;
				frame[i + 1] = g;
				frame[i + 2] = b;
				frame[i + 3] = 255;
			}
		}
		self.pixels.render().unwrap();
		self.window.request_redraw();
	}
}
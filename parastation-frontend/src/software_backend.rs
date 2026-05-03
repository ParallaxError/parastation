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
use parastation_core::gpu::{Colour, DrawParams, FlatVertex, Line, Mask, Polygon, Rect, RectSize, Vertex};

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

impl SoftwareGpuBackend {
	// Polygon drawing
	fn draw_flat_triangle(
		&mut self,
		v0: FlatVertex,
		v1: FlatVertex,
		v2: FlatVertex,
		colour: Colour,
		_semi_transparent: bool,
		draw_params: &DrawParams,
	) {
		let mut verts = [v0.vertex, v1.vertex, v2.vertex];
		verts.sort_unstable_by_key(|v| v.y);
		let [top, mid, bot] = verts;

		if top.y == bot.y { return; }
		if (bot.x - top.x).abs() > 1023 || (bot.y - top.y).abs() > 511 { return; }

		let ox = draw_params.drawing_offset.x as i32;
		let oy = draw_params.drawing_offset.y as i32;
		let area = &draw_params.drawing_area;

		let top_x = top.x as f32;
		let top_y = top.y as f32;
		let mid_x = mid.x as f32;
		let mid_y = mid.y as f32;
		let bot_x = bot.x as f32;
		let bot_y = bot.y as f32;
		let total_dy = bot_y - top_y;

		for y in top.y as i32..=bot.y as i32 {
			let screen_y = y + oy;
			if screen_y < area.y1 as i32 || screen_y > area.y2 as i32 { continue; }
			if screen_y < 0 || screen_y >= 512 { continue; }

			let t = (y as f32 - top_y) / total_dy;
			let long_x = top_x + t * (bot_x - top_x);

			let short_x = if (y as f32) < mid_y {
				let s = (y as f32 - top_y) / (mid_y - top_y);
				top_x + s * (mid_x - top_x)
			} else {
				if bot_y == mid_y { long_x } else {
					let s = (y as f32 - mid_y) / (bot_y - mid_y);
					mid_x + s * (bot_x - mid_x)
				}
			};

			let (x0, x1) = if long_x < short_x {
				(long_x as i32, short_x as i32)
			} else {
				(short_x as i32, long_x as i32)
			};

			for x in x0..=x1 {
				let screen_x = x + ox;
				if screen_x < area.x1 as i32 || screen_x > area.x2 as i32 { continue; }
				if screen_x < 0 || screen_x >= 1024 { continue; }

				self.set_pixel_masked(screen_x as u16, screen_y as u16, colour, &draw_params.mask);
			}
		}
	}
}

impl GpuBackend for SoftwareGpuBackend {
	fn draw_polygon(&mut self, polygon: &Polygon, params: &DrawParams) {
		match polygon {
			Polygon::Monochrome { colour, vertices, semi_transparent } => {
				vertices.triangles(|v0, v1, v2| {
					self.draw_flat_triangle(v0, v1, v2, *colour, *semi_transparent, params);
				});
			}
			_ => { println!("draw_polygon: {:#?}\nparams: {:#?}", polygon, params); }
		}
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

	fn clear_cache(&mut self) {
		println!("clear_cache");
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
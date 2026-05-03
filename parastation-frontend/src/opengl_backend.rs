/*
 * @file /parastation-frontend/src/opengl_backend.rs
 * @brief
 * OpenGL GPU backend implementation, using the glutin crate to render graphics.
 * 
 * -----
 */

// Imports
use glow::{Buffer, Context, Framebuffer, HasContext, Program, RGBA, Texture, VertexArray};
use glutin::surface::WindowSurface;

use parastation_core::gpu::backend::GpuBackend;
use parastation_core::gpu::*;

/// VRAM transfer from the CPU to the OpenGL VRAM texture, accumulating pixels until the specified width 
/// and height are reached, then uploading to the GPU in one go
struct VramTransfer {
    x: u16, y: u16,
    w: u16, h: u16,
    current_x: u16,
    current_y: u16,
    pixels: Vec<u8>,  // RGBA8 accumulated pixels
}

pub struct OpenGlBackend {
    gl: Context,
    // 1024 * 512 RGBA texture on the GPU
    vram_texture: Texture,
    vram_framebuffer: Framebuffer,

    // Shader programs
    flat_program: Program,
    textured_program: Program,
    present_program: Program,

    // Vertex buffer for batching draw calls
    vertex_buffer: Buffer,
    vertex_array: VertexArray,
    present_vao: VertexArray,

    // Display dimensions for letterboxing calculations
    window_width: u32,
    window_height: u32,

    // VRAM transfer
    vram_transfer: Option<VramTransfer>,
}

unsafe fn compile_program(gl: &glow::Context, vert_src: &str, frag_src: &str) -> glow::Program {
    unsafe {
        // Compile vertex shader
        let vert = gl.create_shader(glow::VERTEX_SHADER).unwrap();
        gl.shader_source(vert, vert_src);
        gl.compile_shader(vert);
        if !gl.get_shader_compile_status(vert) {
            panic!("Vertex shader compile error:\n{}", gl.get_shader_info_log(vert));
        }

        // Compile fragment shader
        let frag = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
        gl.shader_source(frag, frag_src);
        gl.compile_shader(frag);
        if !gl.get_shader_compile_status(frag) {
            panic!("Fragment shader compile error:\n{}", gl.get_shader_info_log(frag));
        }

        // Link program
        let program = gl.create_program().unwrap();
        gl.attach_shader(program, vert);
        gl.attach_shader(program, frag);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            panic!("Program link error:\n{}", gl.get_program_info_log(program));
        }

        // Clean up
        gl.detach_shader(program, vert);
        gl.detach_shader(program, frag);
        gl.delete_shader(vert);
        gl.delete_shader(frag);

        program
    }
}

unsafe fn check_gl_errors(gl: &glow::Context, label: &str) {
    unsafe {
        loop {
            let err = gl.get_error();
            if err == glow::NO_ERROR { break; }
            eprintln!("GL error at {label}: {err:#x}");
        }
    }
}

impl OpenGlBackend {
    const FLAT_VERT : &'static str = include_str!("shaders/flat.vert");
    const FLAT_FRAG : &'static str = include_str!("shaders/flat.frag");
    const TEXTURED_VERT : &'static str = include_str!("shaders/textured.vert");
    const TEXTURED_FRAG : &'static str = include_str!("shaders/textured.frag");
    const PRESENT_VERT : &'static str = include_str!("shaders/present.vert");
    const PRESENT_FRAG : &'static str = include_str!("shaders/present.frag");

    pub fn new(gl: Context) -> Self {
        unsafe {
            // Create 1024x512 RGBA texture for VRAM
            let vram_texture = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(vram_texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0,
                glow::RGBA as i32,
                1024, 512, 0,
                glow::RGBA, glow::UNSIGNED_BYTE, 
                None
            );

            // Some filtering to make it look better when scaled up, but still pixelated
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);

            // Create framebuffer for rendering to VRAM texture
            let vram_framebuffer = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(vram_framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, 
                glow::TEXTURE_2D, Some(vram_texture), 0
            );

            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                panic!("FBO incomplete: {status:#x}");
            }

            // Compile our shaders
            // Read source first
            let flat_vert_src = Self::FLAT_VERT;
            let flat_frag_src = Self::FLAT_FRAG;
            let textured_vert_src = Self::TEXTURED_VERT;
            let textured_frag_src = Self::TEXTURED_FRAG;
            let present_vert_src = Self::PRESENT_VERT;
            let present_frag_src = Self::PRESENT_FRAG;

            let flat_program = compile_program(&gl, &flat_vert_src, &flat_frag_src);
            let textured_program = compile_program(&gl, &textured_vert_src, &textured_frag_src);
            let present_program = compile_program(&gl, &present_vert_src, &present_frag_src);

            // Create VAO and VBO
            let vertex_array = gl.create_vertex_array().unwrap();
            let vertex_buffer = gl.create_buffer().unwrap();
            let present_vao = gl.create_vertex_array().unwrap();

            // Set up VAO, recording attribute layout
            gl.bind_vertex_array(Some(vertex_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));

            let stride = std::mem::size_of::<FlatGlVertex>() as i32;
            // location 0 = position (x, y) — 2 floats at offset 0
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
            // location 1 = colour (r, g, b) — 3 floats at offset 8 (after 2 x f32)
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 8);

            // unbind
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            check_gl_errors(&gl, "OpenGlBackend::new");

            Self {
                gl,
                vram_texture,
                vram_framebuffer,
                flat_program,
                textured_program,
                present_program,
                present_vao: present_vao,
                vertex_buffer,
                vertex_array,
                window_width: 1024,
                window_height: 512,
                vram_transfer: None,
            }
        }
    }
}

// Drawing functions
// Mostly just basic OpenGL stuff
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FlatGlVertex {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

unsafe impl bytemuck::Pod for FlatGlVertex {}
unsafe impl bytemuck::Zeroable for FlatGlVertex {}

impl FlatGlVertex {
    fn new(x: i16, y: i16, colour: Colour) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
            r: colour.r as f32,
            g: colour.g as f32,
            b: colour.b as f32,
        }
    }
}

impl OpenGlBackend {
    fn submit_flat(&mut self, verts: &[FlatGlVertex], mode: u32) {
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.vram_framebuffer));
            self.gl.viewport(0, 0, 1024, 512);

            self.gl.use_program(Some(self.flat_program));
            self.gl.bind_vertex_array(Some(self.vertex_array));

            // upload vertices — VAO already knows the layout
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertex_buffer));
            let bytes = bytemuck::cast_slice(verts);
            self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);

            self.gl.draw_arrays(mode, 0, verts.len() as i32);
            check_gl_errors(&self.gl, "submit_flat");
        }
    }

    fn draw_flat_triangle(
        &mut self,
        v0: FlatVertex,
        v1: FlatVertex,
        v2: FlatVertex,
        colour: Colour,
        _semi_transparent: bool,
        params: &DrawParams,
    ) {
        let ox = params.drawing_offset.x;
        let oy = params.drawing_offset.y;

        self.submit_flat(&[
            FlatGlVertex::new(v0.vertex.x + ox, v0.vertex.y + oy, colour),
            FlatGlVertex::new(v1.vertex.x + ox, v1.vertex.y + oy, colour),
            FlatGlVertex::new(v2.vertex.x + ox, v2.vertex.y + oy, colour),
        ], glow::TRIANGLES);
    }

    fn draw_shaded_triangle(
        &mut self,
        v0: ShadedVertex,
        v1: ShadedVertex,
        v2: ShadedVertex,
        _semi_transparent: bool,
        params: &DrawParams,
    ) {
        let ox = params.drawing_offset.x;
        let oy = params.drawing_offset.y;

        self.submit_flat(
            &[
                FlatGlVertex::new(v0.vertex.x + ox, v0.vertex.y + oy, v0.colour),
                FlatGlVertex::new(v1.vertex.x + ox, v1.vertex.y + oy, v1.colour),
                FlatGlVertex::new(v2.vertex.x + ox, v2.vertex.y + oy, v2.colour),
            ],
            glow::TRIANGLES
        );
    }

    fn draw_flat_quad(
        &mut self,
        v0: FlatVertex,
        v1: FlatVertex,
        v2: FlatVertex,
        v3: FlatVertex,
        colour: Colour,
        _semi_transparent: bool,
        params: &DrawParams,
    ) {
        let ox = params.drawing_offset.x;
        let oy = params.drawing_offset.y;

        // PS1 splits quads as (v0,v1,v2) and (v1,v2,v3)
        self.submit_flat(&[
            FlatGlVertex::new(v0.vertex.x + ox, v0.vertex.y + oy, colour),
            FlatGlVertex::new(v1.vertex.x + ox, v1.vertex.y + oy, colour),
            FlatGlVertex::new(v2.vertex.x + ox, v2.vertex.y + oy, colour),
            FlatGlVertex::new(v1.vertex.x + ox, v1.vertex.y + oy, colour),
            FlatGlVertex::new(v2.vertex.x + ox, v2.vertex.y + oy, colour),
            FlatGlVertex::new(v3.vertex.x + ox, v3.vertex.y + oy, colour),
        ], glow::TRIANGLES);
    }
}

impl GpuBackend for OpenGlBackend {
	fn draw_polygon(&mut self, polygon: &Polygon, params: &DrawParams) {
		match polygon {
			Polygon::Monochrome { colour, vertices, semi_transparent } => {
				vertices.triangles(|v0, v1, v2| {
					self.draw_flat_triangle(v0, v1, v2, *colour, *semi_transparent, params);
				});
			}
            Polygon::Shaded { vertices, semi_transparent } => {
                vertices.triangles(|v0, v1, v2| {
                    self.draw_shaded_triangle(v0, v1, v2, *semi_transparent, params);
                });
            }
			_ => { println!("draw_polygon: {:#?}\nparams: {:#?}", polygon, params); }
		}
	}

	fn draw_line(&mut self, line: &Line, params: &DrawParams) {
		println!("draw_line: {:#?}\nparams: {:#?}", line, params);
	}

	fn draw_rect(&mut self, rect: &Rect, params: &DrawParams) {
        let flat = |x, y| FlatVertex { vertex: Vertex { x, y } };
        let yellow = Colour { r: 255, g: 255, b: 0 };
        // self.draw_flat_quad(flat(0, 0), flat(500, 100), flat(100, 400), flat(500, 400), yellow, false, params);

		match rect {
            Rect::Monochrome { colour, pos, size, semi_transparent } => {
                let (w, h) = match size {
                    RectSize::Variable { w, h } => (*w as i16, *h as i16),
                    RectSize::Fixed1x1   => (1, 1),
                    RectSize::Fixed8x8   => (8, 8),
                    RectSize::Fixed16x16 => (16, 16),
                };
                let x0 = pos.x;
                let y0 = pos.y;
                let x1 = x0 + w;
                let y1 = y0 + h;
                let flat = |x, y| FlatVertex { vertex: Vertex { x, y } };
                self.draw_flat_quad(
                    flat(x0, y0), flat(x1, y0),
                    flat(x0, y1), flat(x1, y1),
                    *colour, *semi_transparent, params,
                );
            }
            _ => eprintln!("draw_rect variant not implemented: {rect:#?}"),
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
		self.vram_transfer = Some(VramTransfer {
            x: vram_x, y: vram_y,
            w, h,
            current_x: vram_x,
            current_y: vram_y,
            pixels: Vec::with_capacity(w as usize * h as usize * 4),
        });
	}

	fn vram_write(&mut self, word: u32) {
		let Some(transfer) = &mut self.vram_transfer else { return };

        // each word contains two BGR555 pixels
        for pixel in [word as u16, (word >> 16) as u16] {
            if transfer.current_y >= transfer.y + transfer.h { break; }

            // convert BGR555 to RGBA8
            let r = ((pixel & 0x001F) << 3) as u8;
            let g = (((pixel >> 5) & 0x1F) << 3) as u8;
            let b = (((pixel >> 10) & 0x1F) << 3) as u8;
            let a = 255u8;
            transfer.pixels.extend_from_slice(&[r, g, b, a]);

            transfer.current_x += 1;
            if transfer.current_x >= transfer.x + transfer.w {
                transfer.current_x = transfer.x;
                transfer.current_y += 1;
            }
        }

        // check if transfer is complete
        let complete = transfer.current_y >= transfer.y + transfer.h;
        if complete {
            let x = transfer.x as i32;
            let y = transfer.y as i32;
            let w = transfer.w as i32;
            let h = transfer.h as i32;
            let pixels = transfer.pixels.clone();
            self.vram_transfer = None;

            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.vram_texture));
                self.gl.tex_sub_image_2d(
                    glow::TEXTURE_2D, 0,
                    x, y, w, h,
                    glow::RGBA, glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(&pixels),
                );
            }
        }
	}
	
	fn present(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16) {
        let vram_x = 0u16;
        let vram_y = 0u16;
        let w = 1024u16;
        let h = 512u16;
		unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            
            // get actual window size
            let win_w = self.window_width as f32;
            let win_h = self.window_height as f32;
            
            // compute letterbox — fit w x h into window keeping aspect ratio
            let src_aspect = w as f32 / h as f32;
            let win_aspect = win_w / win_h;
            
            let (dest_w, dest_h, dest_x, dest_y) = if win_aspect > src_aspect {
                // window is wider — pillarbox (black bars on sides)
                let scaled_w = win_h * src_aspect;
                let x_offset = (win_w - scaled_w) / 2.0;
                (scaled_w, win_h, x_offset, 0.0)
            } else {
                // window is taller — letterbox (black bars top/bottom)
                let scaled_h = win_w / src_aspect;
                let y_offset = (win_h - scaled_h) / 2.0;
                (win_w, scaled_h, 0.0, y_offset)
            };

            // set viewport to full window, clear to black
            self.gl.viewport(0, 0, self.window_width as i32, self.window_height as i32);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);

            // convert pixel coords to NDC
            let ndc_x = (dest_x / win_w) * 2.0 - 1.0;
            let ndc_y = 1.0 - (dest_y / win_h) * 2.0;  // flip Y
            let ndc_w = (dest_w / win_w) * 2.0;
            let ndc_h = (dest_h / win_h) * 2.0;

            self.gl.use_program(Some(self.present_program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.vram_texture));

            let loc = |name| self.gl.get_uniform_location(self.present_program, name);
            self.gl.uniform_1_i32(loc("vram").as_ref(), 0);
            self.gl.uniform_2_f32(loc("display_origin").as_ref(),
                vram_x as f32 / 1024.0, vram_y as f32 / 512.0);
            self.gl.uniform_2_f32(loc("display_size").as_ref(),
                w as f32 / 1024.0, h as f32 / 512.0);
            self.gl.uniform_2_f32(loc("screen_offset").as_ref(), ndc_x, ndc_y - ndc_h);
            self.gl.uniform_2_f32(loc("screen_size").as_ref(), ndc_w, ndc_h);

            self.gl.bind_vertex_array(Some(self.present_vao));
            self.gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            check_gl_errors(&self.gl, "present");
        }
	}
}
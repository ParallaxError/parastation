/*
 * @file /parastation-frontend/src/opengl_backend.rs
 * @brief
 * OpenGL GPU backend implementation, using the glutin crate to render graphics.
 *
 * -----
 */

// Imports
use glow::{Buffer, Context, Framebuffer, HasContext, Program, Texture, VertexArray};

use parastation_core::gpu::TextureWindow;
use parastation_core::gpu::backend::GpuBackend;
use parastation_core::gpu::*;

/// VRAM transfer from the CPU to the OpenGL VRAM texture, accumulating pixels until the specified width
/// and height are reached, then uploading to the GPU in one go
struct VramTransfer {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    current_x: u16,
    current_y: u16,
    pixels: Vec<u16>, // R16UI accumulated pixels
}

/// VRAM transfer from the OpenGL VRAM texture to the CPU, reading back a region once and then popping pixels
/// out of it one word (2 pixels) at a time
struct VramReadTransfer {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    pixels: Vec<u16>, // raw BGR555 pixels, read back from vram_texture
    cursor: usize,    // index into pixels of the next pixel to return
}

// Structs for caching uniform locations for each shader program, to avoid repeated glGetUniformLocation calls
struct FlatUniforms {
    drawing_offset: Option<glow::UniformLocation>,
}

struct TexturedUniforms {
    vram: Option<glow::UniformLocation>,
    is_semi_transparent: Option<glow::UniformLocation>,
    is_raw_texture: Option<glow::UniformLocation>,
    tex_page: Option<glow::UniformLocation>,
    tex_window: Option<glow::UniformLocation>,
    tex_depth: Option<glow::UniformLocation>,
    drawing_offset: Option<glow::UniformLocation>,
}

struct PresentUniforms {
    vram: Option<glow::UniformLocation>,
    display_origin: Option<glow::UniformLocation>,
    display_size: Option<glow::UniformLocation>,
    screen_offset: Option<glow::UniformLocation>,
    screen_size: Option<glow::UniformLocation>,
}

pub struct OpenGlBackend {
    gl: Context,
    // 1024 * 512 R16UI texture on the GPU
    vram_texture: Texture,
    vram_framebuffer: Framebuffer,
    // Shader programs
    flat_program: Program,
    textured_program: Program,
    present_program: Program,

    // Vertex buffer for batching draw calls
    vertex_buffer: Buffer,
    vertex_array: VertexArray,
    textured_vertex_buffer: Buffer,
    textured_vertex_array: VertexArray,
    present_vao: VertexArray,

    // Display dimensions for letterboxing calculations
    window_width: u32,
    window_height: u32,

    // VRAM transfer
    vram_transfer: Option<VramTransfer>,
    vram_read_transfer: Option<VramReadTransfer>,

    // Cached uniforms
    flat_uniforms: FlatUniforms,
    textured_uniforms: TexturedUniforms,
    present_uniforms: PresentUniforms,
}

unsafe fn compile_program(gl: &glow::Context, vert_src: &str, frag_src: &str) -> glow::Program {
    unsafe {
        // Compile vertex shader
        let vert = gl.create_shader(glow::VERTEX_SHADER).unwrap();
        gl.shader_source(vert, vert_src);
        gl.compile_shader(vert);
        if !gl.get_shader_compile_status(vert) {
            panic!(
                "Vertex shader compile error:\n{}",
                gl.get_shader_info_log(vert)
            );
        }

        // Compile fragment shader
        let frag = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
        gl.shader_source(frag, frag_src);
        gl.compile_shader(frag);
        if !gl.get_shader_compile_status(frag) {
            panic!(
                "Fragment shader compile error:\n{}",
                gl.get_shader_info_log(frag)
            );
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
    // Ignore gl_errors for now... this DESTROYS performance
    return;
    unsafe {
        loop {
            let err = gl.get_error();
            if err == glow::NO_ERROR {
                break;
            }
            panic!("GL error at {label}: {err:#x}");
        }
    }
}

impl OpenGlBackend {
    const FLAT_VERT: &'static str = include_str!("shaders/flat.vert");
    const FLAT_FRAG: &'static str = include_str!("shaders/flat.frag");
    const TEXTURED_VERT: &'static str = include_str!("shaders/textured.vert");
    const TEXTURED_FRAG: &'static str = include_str!("shaders/textured.frag");
    const PRESENT_VERT: &'static str = include_str!("shaders/present.vert");
    const PRESENT_FRAG: &'static str = include_str!("shaders/present.frag");

    pub fn new(gl: Context) -> Self {
        unsafe {
            // Create 1024x512 R16UI texture for VRAM
            let vram_texture = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(vram_texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::R16UI as i32,
                1024,
                512,
                0,
                glow::RED_INTEGER,
                glow::UNSIGNED_SHORT,
                None,
            );
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

            // Then the framebuffer, will directly present the R16UI texture to the screen
            let vram_framebuffer = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(vram_framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(vram_texture),
                0,
            );

            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                panic!("FBO incomplete: {status:#x}");
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

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
            let textured_vertex_array = gl.create_vertex_array().unwrap();
            let textured_vertex_buffer = gl.create_buffer().unwrap();
            let present_vao = gl.create_vertex_array().unwrap();

            // Set up VAO, recording attribute layout
            gl.bind_vertex_array(Some(vertex_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));

            let stride = std::mem::size_of::<FlatGlVertex>() as i32;
            // location 0 = position (x, y), 2 floats at offset 0
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
            // location 1 = colour (r, g, b), 3 floats at offset 8 (after two floats for position)
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 8);

            // unbind
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            // Next same for textured shaders
            gl.bind_vertex_array(Some(textured_vertex_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(textured_vertex_buffer));

            let stride = std::mem::size_of::<TexturedGlVertex>() as i32;
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0); // position
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 8); // colour
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, stride, 20); // texcoord
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_f32(3, 2, glow::FLOAT, false, stride, 28); // clut

            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            check_gl_errors(&gl, "OpenGlBackend::new");

            // Finally uniform locations, cached for performance
            let flat_uniforms = FlatUniforms {
                drawing_offset: gl.get_uniform_location(flat_program, "drawing_offset"),
            };

            let textured_uniforms = TexturedUniforms {
                vram: gl.get_uniform_location(textured_program, "vram"),
                is_semi_transparent: gl
                    .get_uniform_location(textured_program, "is_semi_transparent"),
                is_raw_texture: gl.get_uniform_location(textured_program, "is_raw_texture"),
                tex_page: gl.get_uniform_location(textured_program, "tex_page"),
                tex_window: gl.get_uniform_location(textured_program, "tex_window"),
                tex_depth: gl.get_uniform_location(textured_program, "tex_depth"),
                drawing_offset: gl.get_uniform_location(textured_program, "drawing_offset"),
            };

            let present_uniforms = PresentUniforms {
                vram: gl.get_uniform_location(present_program, "vram"),
                display_origin: gl.get_uniform_location(present_program, "display_origin"),
                display_size: gl.get_uniform_location(present_program, "display_size"),
                screen_offset: gl.get_uniform_location(present_program, "screen_offset"),
                screen_size: gl.get_uniform_location(present_program, "screen_size"),
            };

            Self {
                gl,
                vram_texture,
                vram_framebuffer,
                flat_program,
                textured_program,
                present_program,
                vertex_buffer,
                vertex_array,
                textured_vertex_buffer,
                textured_vertex_array,
                present_vao: present_vao,
                window_width: 1024,
                window_height: 512,
                vram_transfer: None,
                vram_read_transfer: None,
                flat_uniforms,
                textured_uniforms,
                present_uniforms,
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

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TexturedGlVertex {
    pub x: f32, // position
    pub y: f32,
    pub r: f32, // colour
    pub g: f32,
    pub b: f32,
    pub u: f32, // texcoord in VRAM space
    pub v: f32,
    pub clut_x: f32, // CLUT location in VRAM
    pub clut_y: f32,
}

unsafe impl bytemuck::Pod for TexturedGlVertex {}
unsafe impl bytemuck::Zeroable for TexturedGlVertex {}

impl OpenGlBackend {
    fn submit_flat(
        &mut self,
        verts: &[FlatGlVertex],
        mode: u32,
        drawing_area: &DrawingArea,
        drawing_offset: &DrawingOffset,
    ) {
        unsafe {
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.vram_framebuffer));
            self.gl.viewport(0, 0, 1024, 512);

            // Scissor clip to drawing area
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                drawing_area.x1 as i32,
                drawing_area.y1 as i32,
                (drawing_area.x2 - drawing_area.x1 + 1) as i32,
                (drawing_area.y2 - drawing_area.y1 + 1) as i32,
            );

            self.gl.use_program(Some(self.flat_program));
            self.gl.bind_vertex_array(Some(self.vertex_array));

            self.gl.uniform_2_f32(
                self.flat_uniforms.drawing_offset.as_ref(),
                drawing_offset.x as f32,
                drawing_offset.y as f32,
            );

            // Upload vertex data to GPU
            self.gl
                .bind_buffer(glow::ARRAY_BUFFER, Some(self.vertex_buffer));
            let bytes = bytemuck::cast_slice(verts);
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);

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
        self.submit_flat(
            &[
                FlatGlVertex::new(v0.vertex.x, v0.vertex.y, colour),
                FlatGlVertex::new(v1.vertex.x, v1.vertex.y, colour),
                FlatGlVertex::new(v2.vertex.x, v2.vertex.y, colour),
            ],
            glow::TRIANGLES,
            &params.drawing_area, &params.drawing_offset
        );
    }

    fn draw_shaded_triangle(
        &mut self,
        v0: ShadedVertex,
        v1: ShadedVertex,
        v2: ShadedVertex,
        _semi_transparent: bool,
        params: &DrawParams,
    ) {
        self.submit_flat(
            &[
                FlatGlVertex::new(v0.vertex.x, v0.vertex.y, v0.colour),
                FlatGlVertex::new(v1.vertex.x, v1.vertex.y, v1.colour),
                FlatGlVertex::new(v2.vertex.x, v2.vertex.y, v2.colour),
            ],
            glow::TRIANGLES,
            &params.drawing_area, &params.drawing_offset
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
        // PS1 splits quads as (v0,v1,v2) and (v1,v2,v3)
        self.submit_flat(
            &[
                FlatGlVertex::new(v0.vertex.x, v0.vertex.y, colour),
                FlatGlVertex::new(v1.vertex.x, v1.vertex.y, colour),
                FlatGlVertex::new(v2.vertex.x, v2.vertex.y, colour),
                FlatGlVertex::new(v1.vertex.x, v1.vertex.y, colour),
                FlatGlVertex::new(v2.vertex.x, v2.vertex.y, colour),
                FlatGlVertex::new(v3.vertex.x, v3.vertex.y, colour),
            ],
            glow::TRIANGLES,
            &params.drawing_area, &params.drawing_offset
        );
    }

    fn submit_textured(
        &mut self,
        verts: &[TexturedGlVertex],
        mode: u32,
        tex_depth: i32,
        tex_x: f32,
        tex_y: f32,
        semi_transparent: bool,
        raw_texture: bool,
        semi_transparency_mode: u8,
        texture_window: &TextureWindow,
        drawing_area: &DrawingArea,
        drawing_offset: &DrawingOffset,
    ) {
        unsafe {
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.vram_framebuffer));
            self.gl.viewport(0, 0, 1024, 512);

            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                drawing_area.x1 as i32,
                drawing_area.y1 as i32,
                (drawing_area.x2 - drawing_area.x1 + 1) as i32,
                (drawing_area.y2 - drawing_area.y1 + 1) as i32,
            );

            self.gl.use_program(Some(self.textured_program));

            self.gl.active_texture(glow::TEXTURE0);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.vram_texture));
            self.gl
                .uniform_1_i32(self.textured_uniforms.vram.as_ref(), 0);

            self.gl.uniform_1_i32(
                self.textured_uniforms.is_semi_transparent.as_ref(),
                semi_transparent as i32,
            );

            self.gl.uniform_1_i32(
                self.textured_uniforms.is_raw_texture.as_ref(),
                raw_texture as i32,
            );

            self.gl
                .uniform_2_f32(self.textured_uniforms.tex_page.as_ref(), tex_x, tex_y);

            self.gl.uniform_4_f32(
                self.textured_uniforms.tex_window.as_ref(),
                (texture_window.texture_window_mask_x as f32) * 8.0,
                (texture_window.texture_window_mask_y as f32) * 8.0,
                (texture_window.texture_window_offset_x as f32) * 8.0,
                (texture_window.texture_window_offset_y as f32) * 8.0,
            );

            self.gl.uniform_2_f32(
                self.textured_uniforms.drawing_offset.as_ref(),
                drawing_offset.x as f32,
                drawing_offset.y as f32,
            );

            self.gl.bind_vertex_array(Some(self.textured_vertex_array));
            self.gl
                .bind_buffer(glow::ARRAY_BUFFER, Some(self.textured_vertex_buffer));
            let bytes = bytemuck::cast_slice(verts);
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);

            self.gl
                .uniform_1_i32(self.textured_uniforms.tex_depth.as_ref(), tex_depth);

            self.gl.disable(glow::DEPTH_TEST);
            self.gl.disable(glow::CULL_FACE);
            self.gl.enable(glow::BLEND);
            self.gl.blend_equation(glow::FUNC_ADD);
            self.gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            self.gl.draw_arrays(mode, 0, verts.len() as i32);

            check_gl_errors(&self.gl, "submit_textured");
        }
    }

    fn draw_textured_triangle(
        &mut self,
        v0: TexturedVertex,
        v1: TexturedVertex,
        v2: TexturedVertex,
        colour: Colour,
        texture_params: &TextureParams,
        semi_transparent: bool,
        params: &DrawParams,
    ) {
        // Texpage offset in VRAM pixels
        let tex_x = (texture_params.tex_page.x as f32) * 64.0;
        let tex_y = if texture_params.tex_page.y {
            256.0
        } else {
            0.0
        };

        // For each vertex lets make a quick func to convert it
        let make_vert = |v: TexturedVertex| TexturedGlVertex {
            x: v.vertex.x as f32,
            y: v.vertex.y as f32,
            r: colour.r as f32,
            g: colour.g as f32,
            b: colour.b as f32,
            u: v.texcoord.u as f32,
            v: v.texcoord.v as f32,
            clut_x: texture_params.clut.x as f32 * 16.0, // CLUT x is in 16-pixel units
            clut_y: texture_params.clut.y as f32,
        };

        let tex_depth = texture_params.tex_page.colour_depth as i32;

        self.submit_textured(
            &[make_vert(v0), make_vert(v1), make_vert(v2)],
            glow::TRIANGLES,
            tex_depth,
            tex_x,
            tex_y,
            semi_transparent,
            texture_params.raw_texture,
            texture_params.tex_page.semi_transparency,
            &params.texture_window,
            &params.drawing_area, &params.drawing_offset
        );
    }

    fn draw_shaded_textured_triangle(
        &mut self,
        v0: ShadedTexturedVertex,
        v1: ShadedTexturedVertex,
        v2: ShadedTexturedVertex,
        texture_params: &TextureParams,
        semi_transparent: bool,
        params: &DrawParams,
    ) {
        // Texpage offset in VRAM pixels
        let tex_x = (texture_params.tex_page.x as f32) * 64.0;
        let tex_y = if texture_params.tex_page.y {
            256.0
        } else {
            0.0
        };

        // For each vertex lets make a quick func to convert it
        let make_vert = |v: ShadedTexturedVertex| TexturedGlVertex {
            x: v.vertex.x as f32,
            y: v.vertex.y as f32,
            r: v.colour.r as f32,
            g: v.colour.g as f32,
            b: v.colour.b as f32,
            u: v.texcoord.u as f32,
            v: v.texcoord.v as f32,
            clut_x: texture_params.clut.x as f32 * 16.0, // CLUT x is in 16-pixel units
            clut_y: texture_params.clut.y as f32,
        };

        let tex_depth = texture_params.tex_page.colour_depth as i32;

        self.submit_textured(
            &[make_vert(v0), make_vert(v1), make_vert(v2)],
            glow::TRIANGLES,
            tex_depth,
            tex_x,
            tex_y,
            semi_transparent,
            texture_params.raw_texture,
            texture_params.tex_page.semi_transparency,
            &params.texture_window,
            &params.drawing_area, &params.drawing_offset
        );
    }

    fn draw_textured_rect(
        &mut self,
        pos: Vertex,
        size_w: i16,
        size_h: i16,
        texcoord: Texcoord,
        clut: Clut,
        colour: Colour,
        raw_texture: bool,
        semi_transparent: bool,
        params: &DrawParams,
    ) {
        let mode = &params.draw_mode;

        let tex_x = mode.texture_base_x as f32 * 64.0;
        let tex_y = if mode.texture_base_y { 256.0 } else { 0.0 };
        let tex_depth = mode.texture_page_colours as i32;

        let u0 = texcoord.u as f32;
        let v0 = texcoord.v as f32;
        let u1 = u0 + size_w as f32;
        let v1 = v0 + size_h as f32;

        // Handle GP1/E1h flip flags by swapping the u/v range
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
        let x1 = x0 + size_w;
        let y1 = y0 + size_h;

        let make_vert = |x: i16, y: i16, u: f32, v: f32| TexturedGlVertex {
            x: x as f32,
            y: y as f32,
            r: colour.r as f32,
            g: colour.g as f32,
            b: colour.b as f32,
            u,
            v,
            clut_x: clut.x as f32 * 16.0,
            clut_y: clut.y as f32,
        };

        let verts = [
            make_vert(x0, y0, u0, v0),
            make_vert(x1, y0, u1, v0),
            make_vert(x0, y1, u0, v1),
            make_vert(x1, y0, u1, v0),
            make_vert(x0, y1, u0, v1),
            make_vert(x1, y1, u1, v1),
        ];

        self.submit_textured(
            &verts,
            glow::TRIANGLES,
            tex_depth,
            tex_x,
            tex_y,
            semi_transparent,
            raw_texture,
            mode.semi_transparency,
            &params.texture_window,
            &params.drawing_area,
            &params.drawing_offset,
        );
    }
}

impl GpuBackend for OpenGlBackend {
    fn draw_polygon(&mut self, polygon: &Polygon, params: &DrawParams) {
        match polygon {
            Polygon::Monochrome {
                colour,
                vertices,
                semi_transparent,
            } => {
                vertices.triangles(|v0, v1, v2| {
                    self.draw_flat_triangle(v0, v1, v2, *colour, *semi_transparent, params);
                });
            }
            Polygon::Shaded {
                vertices,
                semi_transparent,
            } => {
                vertices.triangles(|v0, v1, v2| {
                    self.draw_shaded_triangle(v0, v1, v2, *semi_transparent, params);
                });
            }
            Polygon::Textured {
                colour,
                texture_params,
                vertices,
                semi_transparent,
            } => {
                vertices.triangles(|v0, v1, v2| {
                    self.draw_textured_triangle(
                        v0,
                        v1,
                        v2,
                        *colour,
                        texture_params,
                        *semi_transparent,
                        params,
                    );
                });
            }
            Polygon::ShadedTextured {
                texture_params,
                vertices,
                semi_transparent,
            } => {
                vertices.triangles(|v0, v1, v2| {
                    // For shaded textured, we will just use the colour of the first vertex for now
                    self.draw_shaded_textured_triangle(
                        v0,
                        v1,
                        v2,
                        texture_params,
                        *semi_transparent,
                        params,
                    );
                });
            }
        }
    }

    fn draw_line(&mut self, line: &Line, params: &DrawParams) {
        println!("draw_line: {:#?}\nparams: {:#?}", line, params);
    }

    fn draw_rect(&mut self, rect: &Rect, params: &DrawParams) {
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
                let flat = |x, y| FlatVertex {
                    vertex: Vertex { x, y },
                };

                self.draw_flat_quad(
                    flat(x0, y0),
                    flat(x1, y0),
                    flat(x0, y1),
                    flat(x1, y1),
                    *colour,
                    *semi_transparent,
                    params,
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

                self.draw_textured_rect(
                    *pos,
                    w,
                    h,
                    *texcoord,
                    *clut,
                    *colour,
                    *raw,
                    *semi_transparent,
                    params,
                );
            }
        }
    }

    fn fill_rect(&mut self, pos: Vertex, w: u16, h: u16, colour: Colour) {
        let x0 = pos.x;
        let y0 = pos.y;
        let x1 = x0 + w as i16;
        let y1 = y0 + h as i16;

        let flat = |x, y| FlatGlVertex::new(x, y, colour);

        // No drawing offset, no clip, no semi-transparency
        // Also no drawing area, so need to just submit a drawing area that covers the whole VRAM
        let drawing_area = DrawingArea {
            x1: 0,
            y1: 0,
            x2: 1024,
            y2: 512,
        };
        let drawing_offset = DrawingOffset { x: 0, y: 0 };
        self.submit_flat(
            &[
                flat(x0, y0),
                flat(x1, y0),
                flat(x0, y1),
                flat(x1, y0),
                flat(x0, y1),
                flat(x1, y1),
            ],
            glow::TRIANGLES,
            &drawing_area,
            &drawing_offset,
        );
    }

    fn clear_cache(&mut self) {
        // println!("clear_cache");
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
        unsafe {
            // GPU-side texture-to-texture copy, but wont handle mask
            // TODO need to do a shader with blit for copying with mask
            self.gl.copy_image_sub_data(
                self.vram_texture,
                glow::TEXTURE_2D,
                0,
                src_x as i32,
                src_y as i32,
                0,
                self.vram_texture,
                glow::TEXTURE_2D,
                0,
                dst_x as i32,
                dst_y as i32,
                0,
                w as i32,
                h as i32,
                1,
            );

            // TODO handle mask
            let _ = mask;
            if mask.check_mask_before_draw || mask.set_mask_while_drawing {
                panic!("copy_rect: mask check not implemented");
            }

            check_gl_errors(&self.gl, "copy_rect");
        }
    }

    fn vram_read_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16) {
        unsafe {
            // Copy the whole VRAM texture to CPU memory, then extract the requested rectangle
            let mut full = vec![0u16; 1024 * 512];
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.vram_texture));
            self.gl.get_tex_image(
                glow::TEXTURE_2D,
                0,
                glow::RED_INTEGER,
                glow::UNSIGNED_SHORT,
                glow::PixelPackData::Slice(bytemuck::cast_slice_mut(&mut full)),
            );

            // Extract the requested rectangle into a new vector
            let mut pixels = vec![0u16; w as usize * h as usize];
            for row in 0..h as usize {
                for col in 0..w as usize {
                    let sx = vram_x as usize + col;
                    let sy = vram_y as usize + row;
                    pixels[row * w as usize + col] = full[sy * 1024 + sx];
                }
            }

            check_gl_errors(&self.gl, "vram_read_begin");

            self.vram_read_transfer = Some(VramReadTransfer {
                x: vram_x,
                y: vram_y,
                w,
                h,
                pixels,
                cursor: 0,
            });
        }
    }

    fn vram_read(&mut self) -> Option<u32> {
        // return Some(0x12345678u32);
        let transfer = self.vram_read_transfer.as_mut()?;

        let lo = transfer.pixels.get(transfer.cursor).copied().unwrap_or(0);
        transfer.cursor += 1;
        let hi = transfer.pixels.get(transfer.cursor).copied().unwrap_or(0);
        transfer.cursor += 1;

        let word = (lo as u32) | ((hi as u32) << 16);

        if transfer.cursor >= transfer.pixels.len() {
            self.vram_read_transfer = None;
        }

        Some(word)
    }

    fn vram_write_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16, mask: &Mask) {
        self.vram_transfer = Some(VramTransfer {
            x: vram_x,
            y: vram_y,
            w,
            h,
            current_x: vram_x,
            current_y: vram_y,
            pixels: Vec::with_capacity(w as usize * h as usize),
        });
    }

    fn vram_write(&mut self, word: u32) {
        let Some(transfer) = &mut self.vram_transfer else {
            return;
        };

        // Accumulate pixels in the transfer buffer, and when we reach the end of the rectangle, upload to GPU
        for pixel in [word as u16, (word >> 16) as u16] {
            if transfer.current_y >= transfer.y + transfer.h {
                break;
            }
            transfer.pixels.push(pixel);
            transfer.current_x += 1;
            if transfer.current_x >= transfer.x + transfer.w {
                transfer.current_x = transfer.x;
                transfer.current_y += 1;
            }
        }

        let complete = transfer.current_y >= transfer.y + transfer.h;
        if complete {
            let x = transfer.x as i32;
            let y = transfer.y as i32;
            let w = transfer.w as i32;
            let h = transfer.h as i32;
            let pixels_u16 = transfer.pixels.clone();
            self.vram_transfer = None;

            // We're done, so upload them directly to the VRAM texture on the GPU
            unsafe {
                // By default OpenGL expects 4-byte alignment for pixel data, but our VRAM is 2 bytes per pixel, so we
                // need to set the unpack alignment to 2
                self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 2);
                self.gl
                    .bind_texture(glow::TEXTURE_2D, Some(self.vram_texture));
                self.gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    x,
                    y,
                    w,
                    h,
                    glow::RED_INTEGER,
                    glow::UNSIGNED_SHORT,
                    glow::PixelUnpackData::Slice(bytemuck::cast_slice(&pixels_u16)),
                );
                self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
                check_gl_errors(&self.gl, "vram_texture upload");
            }
        }
    }

    fn present(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16) {
        // let vram_x = 0;
        // let vram_y = 0;
        // let w = 1024;
        // let h = 512;
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gl.disable(glow::SCISSOR_TEST);

            let win_w = self.window_width as f32;
            let win_h = self.window_height as f32;

            // We want to scale the 4:3 PS1 output to fit the window, while maintaining aspect ratio
            let target_aspect = 4.0 / 3.0;
            let win_aspect = win_w / win_h;

            let (dest_w, dest_h, dest_x, dest_y) = if win_aspect > target_aspect {
                let scaled_w = win_h * target_aspect;
                let x_offset = (win_w - scaled_w) / 2.0;
                (scaled_w, win_h, x_offset, 0.0)
            } else {
                let scaled_h = win_w / target_aspect;
                let y_offset = (win_h - scaled_h) / 2.0;
                (win_w, scaled_h, 0.0, y_offset)
            };

            self.gl.viewport(0, 0, self.window_width as i32, self.window_height as i32);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);

            let ndc_x = (dest_x / win_w) * 2.0 - 1.0;
            let ndc_y = 1.0 - (dest_y / win_h) * 2.0;
            let ndc_w = (dest_w / win_w) * 2.0;
            let ndc_h = (dest_h / win_h) * 2.0;

            self.gl.use_program(Some(self.present_program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.vram_texture));

            self.gl.uniform_1_i32(self.present_uniforms.vram.as_ref(), 0);
            self.gl.uniform_2_f32(
                self.present_uniforms.display_origin.as_ref(),
                vram_x as f32 / 1024.0,
                vram_y as f32 / 512.0,
            );
            self.gl.uniform_2_f32(
                self.present_uniforms.display_size.as_ref(),
                w as f32 / 1024.0,
                h as f32 / 512.0,
            );
            self.gl.uniform_2_f32(self.present_uniforms.screen_offset.as_ref(), ndc_x, ndc_y - ndc_h);
            self.gl.uniform_2_f32(self.present_uniforms.screen_size.as_ref(), ndc_w, ndc_h);

            self.gl.bind_vertex_array(Some(self.present_vao));
            self.gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            check_gl_errors(&self.gl, "present");
        }
    }
}

/*
 * @file /parastation-web/src/webgl_backend/drawing.rs
 * @brief
 * Drawing utilities for the WebGL backend, primarily shader invocation but also primitive batching to provide
 * a nice interface for the GPUBackend trait while abstracting away optimisations.
 *
 *
 * -----
 */

// Imports
use glow::HasContext;
use parastation_core::gpu::{Colour, DisplayOutput, DrawingArea, DrawingOffset};

use super::WebGlBackend;
use super::render_target::{RenderTarget, VRAM_HEIGHT, VRAM_WIDTH};

pub unsafe fn compile_program(gl: &glow::Context, vert_src: &str, frag_src: &str) -> glow::Program {
    unsafe {
        let vert = gl.create_shader(glow::VERTEX_SHADER).unwrap();
        gl.shader_source(vert, vert_src);
        gl.compile_shader(vert);
        if !gl.get_shader_compile_status(vert) {
            panic!(
                "Vertex shader compile error:\n{}",
                gl.get_shader_info_log(vert)
            );
        }

        let frag = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
        gl.shader_source(frag, frag_src);
        gl.compile_shader(frag);
        if !gl.get_shader_compile_status(frag) {
            panic!(
                "Fragment shader compile error:\n{}",
                gl.get_shader_info_log(frag)
            );
        }

        let program = gl.create_program().unwrap();
        gl.attach_shader(program, vert);
        gl.attach_shader(program, frag);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            panic!("Program link error:\n{}", gl.get_program_info_log(program));
        }

        gl.detach_shader(program, vert);
        gl.detach_shader(program, frag);
        gl.delete_shader(vert);
        gl.delete_shader(frag);

        program
    }
}

// Present pipeline
pub struct PresentUniforms {
    pub source: Option<glow::UniformLocation>,
    pub accurate_source: Option<glow::UniformLocation>, 
    pub colour_depth: Option<glow::UniformLocation>,
    pub display_origin: Option<glow::UniformLocation>,
    pub display_size: Option<glow::UniformLocation>,
    pub screen_offset: Option<glow::UniformLocation>,
    pub screen_size: Option<glow::UniformLocation>,
}

pub struct PresentPipeline {
    pub program: glow::Program,
    pub vertex_array: glow::VertexArray,
    pub uniforms: PresentUniforms,
}

const PRESENT_VERT: &str = include_str!("shaders/present.vert");
const PRESENT_FRAG: &str = include_str!("shaders/present.frag");

pub fn create_present_pipeline(gl: &glow::Context) -> PresentPipeline {
    unsafe {
        let program = compile_program(gl, PRESENT_VERT, PRESENT_FRAG);
        let vao = gl.create_vertex_array().unwrap();

        let uniforms = PresentUniforms {
            source: gl.get_uniform_location(program, "source"),
            accurate_source: gl.get_uniform_location(program, "accurate_source"),
            colour_depth: gl.get_uniform_location(program, "colour_depth"),
            display_origin: gl.get_uniform_location(program, "display_origin"),
            display_size: gl.get_uniform_location(program, "display_size"),
            screen_offset: gl.get_uniform_location(program, "screen_offset"),
            screen_size: gl.get_uniform_location(program, "screen_size"),
        };

        PresentPipeline {
            program,
            vertex_array: vao,
            uniforms,
        }
    }
}

pub fn present(
    gl: &glow::Context,
    pipeline: &PresentPipeline,
    enhanced_target: &RenderTarget,
    accurate_target: &RenderTarget,
    canvas_width: u32,
    canvas_height: u32,
    output: &DisplayOutput,
) {
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.disable(glow::SCISSOR_TEST);

        let win_w = canvas_width as f32;
        let win_h = canvas_height as f32;

        gl.viewport(0, 0, canvas_width as i32, canvas_height as i32);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);

        if !output.enabled {
            return;
        }

        let target_aspect = output.display_aspect_ratio;
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

        let ndc_x = (dest_x / win_w) * 2.0 - 1.0;
        let ndc_y = 1.0 - (dest_y / win_h) * 2.0;
        let ndc_w = (dest_w / win_w) * 2.0;
        let ndc_h = (dest_h / win_h) * 2.0;

        gl.use_program(Some(pipeline.program));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(enhanced_target.texture));

        gl.active_texture(glow::TEXTURE1);
        gl.bind_texture(glow::TEXTURE_2D, Some(accurate_target.texture));
        gl.uniform_1_i32(pipeline.uniforms.accurate_source.as_ref(), 1);

        gl.uniform_1_i32(pipeline.uniforms.source.as_ref(), 0);
        gl.uniform_1_i32(
            pipeline.uniforms.colour_depth.as_ref(),
            output.colour_depth as i32,
        );
        gl.uniform_2_f32(
            pipeline.uniforms.display_origin.as_ref(),
            output.vram_x as f32 / VRAM_WIDTH as f32,
            output.vram_y as f32 / VRAM_HEIGHT as f32,
        );
        gl.uniform_2_f32(
            pipeline.uniforms.display_size.as_ref(),
            output.width_px as f32 / VRAM_WIDTH as f32,
            output.height_px as f32 / VRAM_HEIGHT as f32,
        );
        gl.uniform_2_f32(
            pipeline.uniforms.screen_offset.as_ref(),
            ndc_x,
            ndc_y - ndc_h,
        );
        gl.uniform_2_f32(pipeline.uniforms.screen_size.as_ref(), ndc_w, ndc_h);

        gl.bind_vertex_array(Some(pipeline.vertex_array));
        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
    }
}

// Flat primitives
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FlatGlVertex {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub dither: f32,                 // 0.0 or 1.0
    pub is_semi_transparent: f32,    // 0.0 or 1.0
    pub semi_transparency_mode: f32, // 0.0-3.0
}
unsafe impl bytemuck::Pod for FlatGlVertex {}
unsafe impl bytemuck::Zeroable for FlatGlVertex {}

impl FlatGlVertex {
    pub fn new(
        x: i16,
        y: i16,
        colour: Colour,
        dither: bool,
        is_semi_transparent: bool,
        semi_transparency_mode: u8,
    ) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
            r: colour.r as f32,
            g: colour.g as f32,
            b: colour.b as f32,
            dither: dither as u8 as f32,
            is_semi_transparent: is_semi_transparent as u8 as f32,
            semi_transparency_mode: semi_transparency_mode as f32,
        }
    }
}

pub struct FlatUniforms {
    pub scale: Option<glow::UniformLocation>,
    pub vram_sample: Option<glow::UniformLocation>, // Sampled VRAM texture for semi-transparency
}

pub struct FlatPipeline {
    pub enhanced_program: glow::Program,
    pub accurate_program: glow::Program,
    pub vertex_array: glow::VertexArray,
    pub vertex_buffer: glow::Buffer,
    pub enhanced_uniforms: FlatUniforms,
    pub accurate_uniforms: FlatUniforms,
}

struct DrawFlatParams<'a> {
    target: &'a RenderTarget,
    program: glow::Program,
    uniforms: &'a FlatUniforms,
    scale: f32,
    vram_sample: Option<glow::Texture>,
}

const FLAT_VERT: &str = include_str!("shaders/flat.vert");
const FLAT_FRAG: &str = include_str!("shaders/flat.frag");
const FLAT_ACCURATE_FRAG: &str = include_str!("shaders/flat_accurate.frag");

pub fn create_flat_pipeline(gl: &glow::Context) -> FlatPipeline {
    unsafe {
        let enhanced_program = compile_program(gl, FLAT_VERT, FLAT_FRAG);
        let accurate_program = compile_program(gl, FLAT_VERT, FLAT_ACCURATE_FRAG);

        let vertex_array = gl.create_vertex_array().unwrap();
        let vertex_buffer = gl.create_buffer().unwrap();

        gl.bind_vertex_array(Some(vertex_array));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));

        let stride = std::mem::size_of::<FlatGlVertex>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0); // position
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 8); // colour
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 1, glow::FLOAT, false, stride, 20); // dither
        gl.enable_vertex_attrib_array(3);
        gl.vertex_attrib_pointer_f32(3, 1, glow::FLOAT, false, stride, 24); // semi_transparent
        gl.enable_vertex_attrib_array(4);
        gl.vertex_attrib_pointer_f32(4, 1, glow::FLOAT, false, stride, 28); // semi_transparency_mode

        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);

        let enhanced_uniforms = FlatUniforms {
            scale: gl.get_uniform_location(enhanced_program, "scale"),
            vram_sample: gl.get_uniform_location(enhanced_program, "vram"),
        };
        let accurate_uniforms = FlatUniforms {
            scale: gl.get_uniform_location(accurate_program, "scale"),
            vram_sample: gl.get_uniform_location(accurate_program, "vram"),
        };

        FlatPipeline {
            enhanced_program,
            accurate_program,
            vertex_array,
            vertex_buffer,
            enhanced_uniforms,
            accurate_uniforms,
        }
    }
}

impl WebGlBackend {
    // First time using this in the project, but pub(super) means that this function is public to the current module
    // and its submodules but not outside of the crate
    pub(super) fn submit_flat(
        &self,
        verts: &[FlatGlVertex],
        mode: u32,
        drawing_area: &DrawingArea,
    ) {
        let accurate_params = DrawFlatParams {
            target: &self.accurate_target,
            program: self.flat_pipeline.accurate_program,
            uniforms: &self.flat_pipeline.accurate_uniforms,
            scale: self.accurate_target.width as f32 / VRAM_WIDTH as f32,
            vram_sample: Some(self.accurate_sample.texture),
        };

        let enhanced_scale = self.enhanced_target.width as f32 / VRAM_WIDTH as f32;
        let enhanced_params = DrawFlatParams {
            target: &self.enhanced_target,
            program: self.flat_pipeline.enhanced_program,
            uniforms: &self.flat_pipeline.enhanced_uniforms,
            scale: enhanced_scale,
            vram_sample: Some(self.enhanced_sample.texture),
        };

        for params in [accurate_params, enhanced_params] {
            self.render_flat_to_target(&params, verts, mode, drawing_area);
        }
    }

    fn render_flat_to_target(
        &self,
        params: &DrawFlatParams,
        verts: &[FlatGlVertex],
        mode: u32,
        drawing_area: &DrawingArea,
    ) {
        unsafe {
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(params.target.framebuffer));
            self.gl.viewport(
                0,
                0,
                params.target.width as i32,
                params.target.height as i32,
            );

            self.gl.enable(glow::SCISSOR_TEST);
            let scale = params.scale;
            self.gl.scissor(
                (drawing_area.x1 as f32 * scale) as i32,
                (drawing_area.y1 as f32 * scale) as i32,
                ((drawing_area.x2 - drawing_area.x1 + 1) as f32 * scale) as i32,
                ((drawing_area.y2 - drawing_area.y1 + 1) as f32 * scale) as i32,
            );

            self.gl.use_program(Some(params.program));
            self.gl
                .bind_vertex_array(Some(self.flat_pipeline.vertex_array));
            self.gl.uniform_1_f32(params.uniforms.scale.as_ref(), scale);
            self.gl.active_texture(glow::TEXTURE0);

            if let Some(vram_sample) = params.vram_sample {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(vram_sample));
                self.gl
                    .uniform_1_i32(params.uniforms.vram_sample.as_ref(), 0);
            }

            self.gl
                .bind_buffer(glow::ARRAY_BUFFER, Some(self.flat_pipeline.vertex_buffer));
            let bytes = bytemuck::cast_slice(verts);
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);

            self.gl.draw_arrays(mode, 0, verts.len() as i32);
        }
    }

    pub(super) fn queue_flat(
        &mut self,
        verts: &[FlatGlVertex],
        drawing_area: &DrawingArea,
        drawing_offset: &DrawingOffset,
        mode: u32,
    ) {
        // Flush all textured primitives first since they have a different layout
        self.flush_textured();

        // Semi transparent polygons need a flush as they sample the framebuffer, which is not allowed mid draw call
        if verts.iter().any(|v| v.is_semi_transparent > 0.0) {
            self.flush_flat();
        }
        // If drawing area varies we need to flush the batch before adding the new primitive, since drawing area uses
        // glScissor which can't be changed mid draw call
        else if self.flat_batch.needs_flush_for(drawing_area, mode) {
            self.flush_flat();
        }

        if self.flat_batch.is_empty() {
            self.flat_batch.set_state(*drawing_area, mode);
        }

        let offset_verts: Vec<FlatGlVertex> = verts
            .iter()
            .map(|v| FlatGlVertex {
                x: v.x + drawing_offset.x as f32,
                y: v.y + drawing_offset.y as f32,
                ..*v
            })
            .collect();

        self.flat_batch.push(&offset_verts);
    }

    pub(super) fn flush_flat(&mut self) {
        if self.flat_batch.is_empty() {
            return;
        }
        let Some(drawing_area) = self.flat_batch.drawing_area().copied() else {
            return;
        };

        self.sync_samples();

        self.submit_flat(
            self.flat_batch.verts(),
            self.flat_batch.mode(),
            &drawing_area,
        );

        self.flat_batch.clear();
    }
}

// Textured primitives
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TexturedGlVertex {
    pub x: f32,
    pub y: f32,

    pub r: f32,
    pub g: f32,
    pub b: f32,

    pub u: f32,
    pub v: f32,

    pub clut_x: f32,
    pub clut_y: f32,

    pub tex_page_x: f32,
    pub tex_page_y: f32,
    pub tex_depth: f32,

    pub tex_window_mask_x: f32,
    pub tex_window_mask_y: f32,
    pub tex_window_offset_x: f32,
    pub tex_window_offset_y: f32,

    pub is_raw_texture: f32,         // 0.0 or 1.0
    pub dither: f32,                 // 0.0 or 1.0
    pub semi_transparent: f32,       // 0.0 or 1.0
    pub semi_transparency_mode: f32, // 0.0-3.0
}
unsafe impl bytemuck::Pod for TexturedGlVertex {}
unsafe impl bytemuck::Zeroable for TexturedGlVertex {}

impl TexturedGlVertex {
    pub fn new(
        x: i16,
        y: i16,
        colour: Colour,
        u: f32,
        v: f32,
        clut_x: u16,
        clut_y: u16,
        tex_page_x: u16,
        tex_page_y: u16,
        tex_depth: u8,
        tex_window_mask_x: u8,
        tex_window_mask_y: u8,
        tex_window_offset_x: i8,
        tex_window_offset_y: i8,
        is_raw_texture: bool,
        dither: bool,
        semi_transparent: bool,
        semi_transparency_mode: u8,
    ) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
            r: colour.r as f32,
            g: colour.g as f32,
            b: colour.b as f32,
            u,
            v,
            clut_x: clut_x as f32,
            clut_y: clut_y as f32,
            tex_page_x: tex_page_x as f32,
            tex_page_y: tex_page_y as f32,
            tex_depth: tex_depth as f32,
            tex_window_mask_x: (tex_window_mask_x as f32) * 8.0,
            tex_window_mask_y: (tex_window_mask_y as f32) * 8.0,
            tex_window_offset_x: (tex_window_offset_x as f32) * 8.0,
            tex_window_offset_y: (tex_window_offset_y as f32) * 8.0,
            is_raw_texture: is_raw_texture as u8 as f32,
            dither: dither as u8 as f32,
            semi_transparent: semi_transparent as u8 as f32,
            semi_transparency_mode: semi_transparency_mode as f32,
        }
    }
}

pub struct TexturedUniforms {
    pub scale: Option<glow::UniformLocation>,
    pub vram: Option<glow::UniformLocation>, // 15 bit VRAM sample for texture sampling
    // Sampled VRAM texture for semi-transparency, only for enhanced target (accurate can just use vram)
    pub enhanced_sample: Option<glow::UniformLocation>,
}

pub struct TexturedPipeline {
    pub enhanced_program: glow::Program,
    pub accurate_program: glow::Program,
    pub vertex_array: glow::VertexArray,
    pub vertex_buffer: glow::Buffer,
    pub enhanced_uniforms: TexturedUniforms,
    pub accurate_uniforms: TexturedUniforms,
}

struct DrawTexturedParams<'a> {
    target: &'a RenderTarget,
    program: glow::Program,
    uniforms: &'a TexturedUniforms,
    scale: f32,
    enhanced_sample: Option<glow::Texture>, // Only used for enhanced target
}

const TEXTURED_VERT: &str = include_str!("shaders/textured.vert");
const TEXTURED_FRAG: &str = include_str!("shaders/textured.frag");
const TEXTURED_ACCURATE_FRAG: &str = include_str!("shaders/textured_accurate.frag");

pub fn create_textured_pipeline(gl: &glow::Context) -> TexturedPipeline {
    unsafe {
        let enhanced_program = compile_program(gl, TEXTURED_VERT, TEXTURED_FRAG);
        let accurate_program = compile_program(gl, TEXTURED_VERT, TEXTURED_ACCURATE_FRAG);

        let vertex_array = gl.create_vertex_array().unwrap();
        let vertex_buffer = gl.create_buffer().unwrap();

        gl.bind_vertex_array(Some(vertex_array));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));

        let stride = std::mem::size_of::<TexturedGlVertex>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0); // position
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 8); // colour
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, stride, 20); // uv
        gl.enable_vertex_attrib_array(3);
        gl.vertex_attrib_pointer_f32(3, 2, glow::FLOAT, false, stride, 28); // clut
        gl.enable_vertex_attrib_array(4);
        gl.vertex_attrib_pointer_f32(4, 2, glow::FLOAT, false, stride, 36); // tex_page
        gl.enable_vertex_attrib_array(5);
        gl.vertex_attrib_pointer_f32(5, 1, glow::FLOAT, false, stride, 44); // tex_depth
        gl.enable_vertex_attrib_array(6);
        gl.vertex_attrib_pointer_f32(6, 4, glow::FLOAT, false, stride, 48); // tex_window
        gl.enable_vertex_attrib_array(7);
        gl.vertex_attrib_pointer_f32(7, 1, glow::FLOAT, false, stride, 64); // is_raw_texture
        gl.enable_vertex_attrib_array(8);
        gl.vertex_attrib_pointer_f32(8, 1, glow::FLOAT, false, stride, 68); // dither
        gl.enable_vertex_attrib_array(9);
        gl.vertex_attrib_pointer_f32(9, 1, glow::FLOAT, false, stride, 72); // semi_transparent
        gl.enable_vertex_attrib_array(10);
        gl.vertex_attrib_pointer_f32(10, 1, glow::FLOAT, false, stride, 76); // semi_transparency_mode

        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);

        let enhanced_uniforms = TexturedUniforms {
            scale: gl.get_uniform_location(enhanced_program, "scale"),
            vram: gl.get_uniform_location(enhanced_program, "vram"),
            enhanced_sample: gl.get_uniform_location(enhanced_program, "enhanced_sample"),
        };
        let accurate_uniforms = TexturedUniforms {
            scale: gl.get_uniform_location(accurate_program, "scale"),
            vram: gl.get_uniform_location(accurate_program, "vram"),
            enhanced_sample: None, // Not used in accurate program
        };

        TexturedPipeline {
            enhanced_program,
            accurate_program,
            vertex_array,
            vertex_buffer,
            enhanced_uniforms,
            accurate_uniforms,
        }
    }
}

impl WebGlBackend {
    pub(super) fn submit_textured(
        &self,
        verts: &[TexturedGlVertex],
        mode: u32,
        drawing_area: &DrawingArea,
    ) {
        let accurate_params = DrawTexturedParams {
            target: &self.accurate_target,
            program: self.textured_pipeline.accurate_program,
            uniforms: &self.textured_pipeline.accurate_uniforms,
            scale: self.accurate_target.width as f32 / VRAM_WIDTH as f32,
            enhanced_sample: None, // Not used in accurate program
        };

        let enhanced_scale = self.enhanced_target.width as f32 / VRAM_WIDTH as f32;
        let enhanced_params = DrawTexturedParams {
            target: &self.enhanced_target,
            program: self.textured_pipeline.enhanced_program,
            uniforms: &self.textured_pipeline.enhanced_uniforms,
            scale: enhanced_scale,
            enhanced_sample: Some(self.enhanced_sample.texture),
        };

        for params in [accurate_params, enhanced_params] {
            self.render_textured_to_target(&params, verts, mode, drawing_area);
        }
    }

    fn render_textured_to_target(
        &self,
        params: &DrawTexturedParams,
        verts: &[TexturedGlVertex],
        mode: u32,
        drawing_area: &DrawingArea,
    ) {
        unsafe {
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(params.target.framebuffer));
            self.gl.viewport(
                0,
                0,
                params.target.width as i32,
                params.target.height as i32,
            );

            self.gl.enable(glow::SCISSOR_TEST);
            let scale = params.scale;
            self.gl.scissor(
                (drawing_area.x1 as f32 * scale) as i32,
                (drawing_area.y1 as f32 * scale) as i32,
                ((drawing_area.x2 - drawing_area.x1 + 1) as f32 * scale) as i32,
                ((drawing_area.y2 - drawing_area.y1 + 1) as f32 * scale) as i32,
            );

            self.gl.use_program(Some(params.program));
            self.gl
                .bind_vertex_array(Some(self.textured_pipeline.vertex_array));
            self.gl.uniform_1_f32(params.uniforms.scale.as_ref(), scale);

            if let Some(vram_loc) = &params.uniforms.vram {
                self.gl.active_texture(glow::TEXTURE0);
                self.gl
                    .bind_texture(glow::TEXTURE_2D, Some(self.accurate_sample.texture));
                self.gl.uniform_1_i32(Some(vram_loc), 0);
            }

            if let Some(enhanced_sample_loc) = &params.uniforms.enhanced_sample {
                self.gl.active_texture(glow::TEXTURE1);
                self.gl
                    .bind_texture(glow::TEXTURE_2D, params.enhanced_sample);
                self.gl.uniform_1_i32(Some(enhanced_sample_loc), 1);
            }

            self.gl.bind_buffer(
                glow::ARRAY_BUFFER,
                Some(self.textured_pipeline.vertex_buffer),
            );
            let bytes = bytemuck::cast_slice(verts);
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);

            self.gl.draw_arrays(mode, 0, verts.len() as i32);
        }
    }

    pub(super) fn queue_textured(
        &mut self,
        verts: &[TexturedGlVertex],
        drawing_area: &DrawingArea,
        drawing_offset: &DrawingOffset,
    ) {
        // Flush all flat primitives first since they have a different layout
        self.flush_flat();

        // Semi transparent polygons need a flush as they sample the framebuffer, which is not allowed mid draw call
        if verts.iter().any(|v| v.semi_transparent > 0.0) {
            self.flush_textured();
        }
        // If drawing area varies we need to flush the batch before adding the new primitive, since drawing area uses
        // glScissor which can't be changed mid draw call
        else if self.textured_batch.needs_flush_for(drawing_area) {
            self.flush_textured();
        }

        if self.textured_batch.is_empty() {
            self.textured_batch.set_drawing_area(*drawing_area);
        }

        let offset_verts: Vec<TexturedGlVertex> = verts
            .iter()
            .map(|v| TexturedGlVertex {
                x: v.x + drawing_offset.x as f32,
                y: v.y + drawing_offset.y as f32,
                ..*v
            })
            .collect();

        self.textured_batch.push(&offset_verts);
    }

    pub(super) fn flush_textured(&mut self) {
        if self.textured_batch.is_empty() {
            return;
        }
        let Some(drawing_area) = self.textured_batch.drawing_area().copied() else {
            return;
        };

        self.sync_samples();

        self.submit_textured(self.textured_batch.verts(), glow::TRIANGLES, &drawing_area);

        self.textured_batch.clear();
    }
}

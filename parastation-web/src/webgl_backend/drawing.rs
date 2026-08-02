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

use super::PresentUniforms;
use super::WebGlBackend;
use super::render_target::{RenderTarget, VRAM_HEIGHT, VRAM_WIDTH};

unsafe fn compile_program(gl: &glow::Context, vert_src: &str, frag_src: &str) -> glow::Program {
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

        gl.uniform_1_i32(pipeline.uniforms.source.as_ref(), 0);
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
        };
        let accurate_uniforms = FlatUniforms {
            scale: gl.get_uniform_location(accurate_program, "scale"),
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
        };

        let enhanced_scale = self.enhanced_target.width as f32 / VRAM_WIDTH as f32;
        let enhanced_params = DrawFlatParams {
            target: &self.enhanced_target,
            program: self.flat_pipeline.enhanced_program,
            uniforms: &self.flat_pipeline.enhanced_uniforms,
            scale: enhanced_scale,
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
    ) {
        // Flush all textured primitives first since they have a different layout
        self.flush_textured();

        // If drawing area varies we need to flush the batch before adding the new primitive, since drawing area uses
        // glScissor which can't be changed mid draw call
        if self.flat_batch.needs_flush_for(drawing_area) {
            self.flush_flat();
        }

        if self.flat_batch.is_empty() {
            self.flat_batch.set_drawing_area(*drawing_area);
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

        self.submit_flat(self.flat_batch.verts(), glow::TRIANGLES, &drawing_area);

        self.flat_batch.clear();
    }
}

// Textured primitives
impl WebGlBackend {
    pub(super) fn queue_textured(
        &mut self,
        _verts: &[FlatGlVertex],
        _drawing_area: &DrawingArea,
        _drawing_offset: &DrawingOffset,
    ) {
        // Flush all flat primitives first since they have a different layout
        self.flush_flat();
    }

    pub(super) fn flush_textured(&mut self) {}
}

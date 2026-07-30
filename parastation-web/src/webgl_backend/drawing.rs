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
use parastation_core::gpu::DisplayOutput;

use super::PresentUniforms;
use super::render_target::{RenderTarget, VRAM_HEIGHT, VRAM_WIDTH};

const PRESENT_VERT: &str = include_str!("shaders/present.vert");
const PRESENT_FRAG: &str = include_str!("shaders/present.frag");

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
pub fn create_present_pipeline(
    gl: &glow::Context,
) -> (glow::Program, glow::VertexArray, PresentUniforms) {
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

        (program, vao, uniforms)
    }
}

pub fn present(
    gl: &glow::Context,
    program: glow::Program,
    vao: glow::VertexArray,
    uniforms: &PresentUniforms,
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

        gl.use_program(Some(program));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(enhanced_target.texture));

        gl.uniform_1_i32(uniforms.source.as_ref(), 0);
        gl.uniform_2_f32(
            uniforms.display_origin.as_ref(),
            output.vram_x as f32 / VRAM_WIDTH as f32,
            output.vram_y as f32 / VRAM_HEIGHT as f32,
        );
        gl.uniform_2_f32(
            uniforms.display_size.as_ref(),
            output.width_px as f32 / VRAM_WIDTH as f32,
            output.height_px as f32 / VRAM_HEIGHT as f32,
        );
        gl.uniform_2_f32(uniforms.screen_offset.as_ref(), ndc_x, ndc_y - ndc_h);
        gl.uniform_2_f32(uniforms.screen_size.as_ref(), ndc_w, ndc_h);

        gl.bind_vertex_array(Some(vao));
        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
    }
}

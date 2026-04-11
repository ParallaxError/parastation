/*
 * @file /parastation-core/src/gpu/backend.rs
 * @brief
 * Backend trait that defines the implementations for GPU commands.
 * The trait isn't implemented by the core as it's frontend dependent, but the appropriate
 * frontend must implement the GPU commands defined by
 * https://problemkaputt.de/psx-spx.htm#gpuioportsdmachannelscommandsvram
 * 
 * -----
 */

// Imports
use crate::gpu::gpu_state::Mask;
use crate::gpu::rendering_parameters::*;

pub trait GpuBackend {
    // Drawing commands
    /// Render a quad or triangle with the given shading parameters and drawing mode
    fn draw_polygon(&mut self, polygon: &Polygon, params: &DrawParams);

    /// Render a line with the given shading parameters and drawing mode
    fn draw_line(&mut self, line: &Line, params: &DrawParams);

    /// Render a rectangle with the given shading parameters and drawing mode
    fn draw_rect(&mut self, rect: &Rect, params: &DrawParams);

    /// Fill a rectangle with a solid colour
    /// Invoked by the GP0 command 0x02
    fn fill_rect(&mut self, pos: Vertex, w: u16, h: u16, colour: Colour);

    // VRAM commands
    /// Copy a rectangular area from VRAM to another area in VRAM
    fn copy_rect(&mut self, src_x: u16, src_y: u16, dst_x: u16, dst_y: u16, w: u16, h: u16, mask: &Mask);

    /// Initiate a DMA transfer from VRAM to the CPU, with words being sent through GPUREAD
    fn vram_read_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16);

    /// Read a word from the VRAM DMA transfer initiated by vram_read_begin, returning None
    /// if done/never initiated
    fn vram_read(&mut self) -> Option<u32>;

    /// Initiate a DMA transfer from the CPU to VRAM, with words being sent through GP0
    fn vram_write_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16, mask: &Mask);

    /// Write a word to the VRAM DMA transfer initiated by vram_write_begin, 
    /// raising an error if done/never initiated
    fn vram_write(&mut self, word: u32);

    // Display methods
    /// Present the display area in VRAM to the screen
    fn present(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16);
}
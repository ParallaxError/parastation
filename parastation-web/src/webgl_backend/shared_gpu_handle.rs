/*
 * @file /parastation-web/src/webgl_backend/shared_gpu_handle.rs
 * @brief
 * Wrwapper around WebGlBackend to allow it to be shared between the PS1 core and the WebRunner while satisfying
 * the GpuBackend trait.
 *
 * -----
 */

use std::cell::RefCell;
use std::rc::Rc;

use parastation_core::gpu::backend::GpuBackend;
use parastation_core::gpu::*;

use super::WebGlBackend;

pub struct SharedGpuHandle(pub Rc<RefCell<WebGlBackend>>);

impl SharedGpuHandle {
    pub fn new(shared: Rc<RefCell<WebGlBackend>>) -> Self {
        Self(shared)
    }
}

macro_rules! delegate_gpu_backend {
    ($self:ident, $method:ident($($arg:ident),*)) => {
        $self.0.borrow_mut().$method($($arg),*)
    };
}

impl GpuBackend for SharedGpuHandle {
    fn draw_polygon(&mut self, polygon: &Polygon, params: &DrawParams) {
        delegate_gpu_backend!(self, draw_polygon(polygon, params))
    }
    fn draw_line(&mut self, line: &Line, params: &DrawParams) {
        delegate_gpu_backend!(self, draw_line(line, params))
    }
    fn draw_rect(&mut self, rect: &Rect, params: &DrawParams) {
        delegate_gpu_backend!(self, draw_rect(rect, params))
    }
    fn fill_rect(&mut self, pos: Vertex, w: u16, h: u16, colour: Colour) {
        delegate_gpu_backend!(self, fill_rect(pos, w, h, colour))
    }
    fn clear_cache(&mut self) {
        delegate_gpu_backend!(self, clear_cache())
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
        delegate_gpu_backend!(self, copy_rect(src_x, src_y, dst_x, dst_y, w, h, mask))
    }
    fn vram_read_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16) {
        delegate_gpu_backend!(self, vram_read_begin(vram_x, vram_y, w, h))
    }
    fn vram_read(&mut self) -> Option<u32> {
        delegate_gpu_backend!(self, vram_read())
    }
    fn vram_write_begin(&mut self, vram_x: u16, vram_y: u16, w: u16, h: u16, mask: &Mask) {
        delegate_gpu_backend!(self, vram_write_begin(vram_x, vram_y, w, h, mask))
    }
    fn vram_write(&mut self, word: u32) {
        delegate_gpu_backend!(self, vram_write(word))
    }
    fn present(&mut self, output: &DisplayOutput) {
        delegate_gpu_backend!(self, present(output))
    }
}

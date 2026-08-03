/*
 * @file /parastation-web/src/webgl_backend/batch.rs
 * @brief
 * Batch of flat/textured vertices to draw in a single draw call, to reduce overhead from sending a draw call for every
 * drawn primitive. Must be flushed at any boundary where the framebuffer is sampled like VRAM reads or present calls.
 *
 * -----
 */

use parastation_core::gpu::DrawingArea;

use super::drawing::{FlatGlVertex, TexturedGlVertex};

pub struct FlatBatch {
    verts: Vec<FlatGlVertex>,
    current_drawing_area: Option<DrawingArea>,
}

impl FlatBatch {
    pub fn new() -> Self {
        Self {
            verts: Vec::new(),
            current_drawing_area: None,
        }
    }

    pub fn push(&mut self, verts: &[FlatGlVertex]) {
        self.verts.extend_from_slice(verts);
    }

    /// Returns true if adding a primitive with this drawing area would require flushing first.
    pub fn needs_flush_for(&self, drawing_area: &DrawingArea) -> bool {
        match &self.current_drawing_area {
            None => false,
            Some(current_area) => current_area != drawing_area,
        }
    }

    pub fn set_drawing_area(&mut self, drawing_area: DrawingArea) {
        self.current_drawing_area = Some(drawing_area);
    }

    pub fn is_empty(&self) -> bool {
        self.verts.is_empty()
    }

    pub fn drawing_area(&self) -> Option<&DrawingArea> {
        self.current_drawing_area.as_ref()
    }

    pub fn verts(&self) -> &[FlatGlVertex] {
        &self.verts
    }

    pub fn clear(&mut self) {
        self.verts.clear();
        self.current_drawing_area = None;
    }
}

pub struct TexturedBatch {
    verts: Vec<TexturedGlVertex>,
    current_drawing_area: Option<DrawingArea>,
}

impl TexturedBatch {
    pub fn new() -> Self {
        Self {
            verts: Vec::new(),
            current_drawing_area: None,
        }
    }

    pub fn push(&mut self, verts: &[TexturedGlVertex]) {
        self.verts.extend_from_slice(verts);
    }

    /// Returns true if adding a primitive with this drawing area would require flushing first.
    pub fn needs_flush_for(&self, drawing_area: &DrawingArea) -> bool {
        match &self.current_drawing_area {
            None => false,
            Some(current_area) => current_area != drawing_area,
        }
    }

    pub fn set_drawing_area(&mut self, drawing_area: DrawingArea) {
        self.current_drawing_area = Some(drawing_area);
    }

    pub fn is_empty(&self) -> bool {
        self.verts.is_empty()
    }

    pub fn drawing_area(&self) -> Option<&DrawingArea> {
        self.current_drawing_area.as_ref()
    }

    pub fn verts(&self) -> &[TexturedGlVertex] {
        &self.verts
    }

    pub fn clear(&mut self) {
        self.verts.clear();
        self.current_drawing_area = None;
    }
}

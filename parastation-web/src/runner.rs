/*
 * @file /parastation-web/src/runner.rs
 * @brief
 * Web frontend runner implementation for ParaStation. Encapsulates the behaviour to run the PS1 at a set framerate
 * while handling I/O events like audio/video output and keyboard input.
 *
 * -----
 */

// Imports
use std::cell::RefCell;
use std::rc::Rc;

use parastation_core::bios::Bios;
use parastation_core::{Interpreter, Ps1};

use crate::dummy_backends::*;
use crate::web_spu_backend::{SharedSpuHandle, WebSpuBackend};
use crate::webgl_backend::{WebGlBackend, create_gl_context};

/// Owns the PS1 core and the backend handles needed for the JavaScript to interact with the emulator. Doesn't own the
/// main loop or the canvas, the JavaScript's responsibility is to call tick_frame() once per requestAnimationFrame tick
/// and drain_audio() to pull audio samples
pub struct WebRunner {
    ps1: Option<Ps1<Interpreter>>,
    spu_handle: Option<Rc<RefCell<WebSpuBackend>>>,
    canvas: web_sys::HtmlCanvasElement,

    total_cycles_run: u64,
    total_frames_run: u64,
}

impl WebRunner {
    pub fn new(canvas: &web_sys::HtmlCanvasElement) -> Self {
        Self {
            ps1: None,
            spu_handle: None,
            canvas: canvas.clone(),
            total_cycles_run: 0,
            total_frames_run: 0,
        }
    }

    pub fn load_bios(&mut self, bios_bytes: Vec<u8>) {
        let bios = Bios::new(bios_bytes.into_boxed_slice());

        let width = self.canvas.width();
        let height = self.canvas.height();
        let gl = create_gl_context(&self.canvas);
        let gpu_backend = Box::new(WebGlBackend::new(gl, width, height));

        let spu_shared = Rc::new(RefCell::new(WebSpuBackend::new()));
        let spu_for_ps1 = Box::new(SharedSpuHandle::new(Rc::clone(&spu_shared)));

        let ps1 = Ps1::new(
            bios,
            Interpreter::new(),
            gpu_backend,
            spu_for_ps1,
            Box::new(DummyInputProvider),
            Box::new(DummyInputProvider),
        );

        self.ps1 = Some(ps1);
        self.spu_handle = Some(spu_shared);
    }

    /// Runs the emulator for the given number of CPU cycles. Called once per requestAnimationFrame tick from JS for as
    /// many cycles as are needed to keep up with the framerate
    pub fn tick_frame(&mut self, cycles: u32) {
        let Some(ps1) = &mut self.ps1 else {
            return; // no BIOS loaded yet
        };

        ps1.run(cycles as u64);
        ps1.display();
        self.total_cycles_run += cycles as u64;
        self.total_frames_run += 1;
    }

    /// Pulls buffered audio samples as interleaved f32 (L, R, L, R, ...) in the range -1 to 1, ready for a web audio
    /// context to consume
    pub fn drain_audio(&mut self, max_frames: usize) -> Vec<f32> {
        let Some(spu_handle) = &self.spu_handle else {
            return Vec::new();
        };
        spu_handle.borrow_mut().drain_interleaved_f32(max_frames)
    }
}

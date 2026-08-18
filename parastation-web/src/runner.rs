/*
 * @file /parastation-web/src/runner.rs
 * @brief
 * Web frontend runner implementation for ParaStation. Runs inside a Worker, encapsulating the behaviour to run the PS1
 * at a set framerate while handling I/O events like audio/video output and keyboard input communicated through
 * postMessage.
 *
 * -----
 */

// Imports
use std::cell::RefCell;
use std::rc::Rc;

use parastation_core::bios::Bios;
use parastation_core::sio0::JoypadButton;
use parastation_core::{Interpreter, Ps1, log};
use std::collections::HashMap;
use web_sys::{File, OffscreenCanvas};

use crate::remappable_input_provider::*;
use crate::web_file::WebFile;
use crate::web_spu_backend::{SharedSpuHandle, WebSpuBackend};
use crate::webgl_backend::shared_gpu_handle::SharedGpuHandle;
use crate::webgl_backend::{WebGlBackend, create_gl_context};

/// Owns the PS1 core and the backend handles needed for the JavaScript to interact with the emulator. Doesn't own the
/// main loop or the canvas, the JavaScript's responsibility is to call tick_frame() once per requestAnimationFrame tick
/// and drain_audio() to pull audio samples
pub struct WebRunner {
    ps1: Option<Ps1<Interpreter>>,
    spu_handle: Option<Rc<RefCell<WebSpuBackend>>>,
    gpu_handle: Option<Rc<RefCell<WebGlBackend>>>,
    controller_1_handle: Option<Rc<RefCell<RemappableInputProvider>>>,
    canvas: OffscreenCanvas,
    scale: u32,

    total_cycles_run: u64,
    total_frames_run: u64,
}

impl WebRunner {
    pub fn new(canvas: OffscreenCanvas, scale: u32) -> Self {
        Self {
            ps1: None,
            spu_handle: None,
            gpu_handle: None,
            controller_1_handle: None,
            canvas: canvas,
            scale: scale,
            total_cycles_run: 0,
            total_frames_run: 0,
        }
    }

    pub fn load_bios(&mut self, bios_bytes: Vec<u8>) {
        let bios = Bios::new(bios_bytes.into_boxed_slice());

        let width = self.canvas.width();
        let height = self.canvas.height();
        let gl = create_gl_context(&self.canvas);

        let gpu_shared = Rc::new(RefCell::new(WebGlBackend::new(
            gl, width, height, self.scale,
        )));
        let gpu_for_ps1 = Box::new(SharedGpuHandle::new(Rc::clone(&gpu_shared)));

        let spu_shared = Rc::new(RefCell::new(WebSpuBackend::new()));
        let spu_for_ps1 = Box::new(SharedSpuHandle::new(Rc::clone(&spu_shared)));

        let controller_1_shared = Rc::new(RefCell::new(RemappableInputProvider::new(
            JoypadState::new(),
            DEFAULT_KEYBOARD_MAPPING,
        )));
        let controller_1_for_ps1 =
            Box::new(SharedInputHandle::new(Rc::clone(&controller_1_shared)));

        let ps1 = Ps1::new(
            bios,
            Interpreter::new(),
            gpu_for_ps1,
            spu_for_ps1,
            controller_1_for_ps1,
            Box::new(DummyInputProvider),
        );

        self.ps1 = Some(ps1);
        self.spu_handle = Some(spu_shared);
        self.gpu_handle = Some(gpu_shared);
        self.controller_1_handle = Some(controller_1_shared);
    }

    /// Runs the emulator for the given number of CPU cycles. Called once per requestAnimationFrame tick from JS for as
    /// many cycles as are needed to keep up with the framerate
    pub fn tick_frame(&mut self, cycles: u32) {
        let Some(ps1) = &mut self.ps1 else {
            return;
        };

        ps1.run(cycles as u64);
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

    // Keyboard/touch generic input methods
    // TODO should press for both controller 1 and 2
    pub fn input_down(&mut self, key_code: &str) {
        if let Some(handle) = &self.controller_1_handle {
            handle.borrow().press(key_code);
        }
    }

    pub fn input_up(&mut self, key_code: &str) {
        if let Some(handle) = &self.controller_1_handle {
            handle.borrow().release(key_code);
        }
    }

    pub fn rebind_input(&mut self, id: String, button: WebJoypadButton) {
        if let Some(handle) = &self.controller_1_handle {
            handle.borrow_mut().rebind(id, button.into());
        }
    }

    /// Inserts a CD-ROM disc into the PS1, given the CUE file content and a mapping of BIN filenames to browser
    /// File handles
    pub fn insert_disc(&mut self, cue_content: &str, bin_files: HashMap<String, File>) {
        let Some(ps1) = &mut self.ps1 else {
            return; // no BIOS loaded yet
        };

        ps1.insert_cdrom_disc(cue_content, &mut |filename: &str| {
            let file = bin_files
                .get(filename)
                .unwrap_or_else(|| panic!("CUE references {filename} but it wasn't provided"));
            Box::new(WebFile::new(file.clone())) as Box<dyn parastation_core::DiscSource>
        });
    }

    /// Save data from the memory card to a byte array, which can then be saved to disk or sent over the network.
    pub fn save_memory_card(&mut self, port: u8) -> Vec<u8> {
        let Some(ps1) = &mut self.ps1 else {
            return Vec::new(); // no BIOS loaded yet
        };
        ps1.save_memory_card(port)
    }

    /// Load data into the memory card from a byte array, which can be loaded from disk or received over the network.
    pub fn load_memory_card(&mut self, port: u8, data: &[u8]) {
        let Some(ps1) = &mut self.ps1 else {
            return; // no BIOS loaded yet
        };
        ps1.load_memory_card(port, data);
    }

    // GPU debug methods
    pub fn dump_accurate_vram(&self) -> Option<(u32, u32, Vec<u8>)> {
        let gpu_handle = self.gpu_handle.as_ref()?;
        Some(gpu_handle.borrow().dump_accurate_target())
    }

    pub fn dump_enhanced_vram(&self) -> Option<(u32, u32, Vec<u8>)> {
        let gpu_handle = self.gpu_handle.as_ref()?;
        Some(gpu_handle.borrow().dump_enhanced_target())
    }

    pub fn dump_accurate_sample(&self) -> Option<(u32, u32, Vec<u8>)> {
        let gpu_handle = self.gpu_handle.as_ref()?;
        Some(gpu_handle.borrow().dump_accurate_sample())
    }

    pub fn dump_enhanced_sample(&self) -> Option<(u32, u32, Vec<u8>)> {
        let gpu_handle = self.gpu_handle.as_ref()?;
        Some(gpu_handle.borrow().dump_enhanced_sample())
    }
}

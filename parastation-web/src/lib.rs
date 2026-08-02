/*
 * @file /parastation-web/src/lib.rs
 * @brief
 * Static webpage frontend for ParaStation.
 * This frontend is intended to be compiled to WebAssembly and run in a browser. The HTML and JS can then interface with
 * all the relevant parts of the emulator through the WebRunner class, which is exposed to JS via wasm-bindgen.
 *
 * -----
 */

mod dummy_backends;
mod runner;
mod web_logger;
mod web_spu_backend;
mod webgl_backend;

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use runner::WebRunner as InnerRunner;

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
    parastation_core::logging::set_logger(Box::new(web_logger::WebLogger::new()));
}

#[wasm_bindgen]
pub struct WebRunner {
    inner: InnerRunner,
}

#[wasm_bindgen]
impl WebRunner {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: HtmlCanvasElement) -> Self {
        Self {
            inner: InnerRunner::new(&canvas),
        }
    }

    pub fn load_bios(&mut self, bios_bytes: Vec<u8>) {
        self.inner.load_bios(bios_bytes);
    }

    pub fn tick_frame(&mut self, cycles: u32) {
        self.inner.tick_frame(cycles);
    }

    pub fn drain_audio(&mut self, max_frames: usize) -> Vec<f32> {
        self.inner.drain_audio(max_frames)
    }

    pub fn dump_accurate_vram(&self) -> Option<js_sys::Uint8Array> {
        self.inner
            .dump_accurate_vram()
            .map(|(_, _, bytes)| js_sys::Uint8Array::from(bytes.as_slice()))
    }

    pub fn dump_enhanced_vram(&self) -> Option<js_sys::Uint8Array> {
        self.inner
            .dump_enhanced_vram()
            .map(|(_, _, bytes)| js_sys::Uint8Array::from(bytes.as_slice()))
    }

    // Pretty stupid way to do this
    pub fn accurate_vram_dims(&self) -> Vec<u32> {
        self.inner
            .dump_accurate_vram()
            .map(|(w, h, _)| vec![w, h])
            .unwrap_or_default()
    }

    pub fn enhanced_vram_dims(&self) -> Vec<u32> {
        self.inner
            .dump_enhanced_vram()
            .map(|(w, h, _)| vec![w, h])
            .unwrap_or_default()
    }
}

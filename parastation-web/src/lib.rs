/*
 * @file /parastation-web/src/lib.rs
 * @brief
 * Worker-side WebAssembly entry point for ParaStation. Runs entirely within a dedicated Worker so that the CD/ROM disc
 * reading can use FileReaderSync, only available for workers. The main thread talks to the worker entirely through
 * postMessage, the JavaScript has details on the interface.
 *
 * -----
 */

mod remappable_input_provider;
mod runner;
mod web_file;
mod web_logger;
mod web_spu_backend;
mod webgl_backend;

use js_sys::Map;
use wasm_bindgen::prelude::*;
use web_sys::{File, OffscreenCanvas};

use runner::WebRunner as InnerRunner;

use crate::remappable_input_provider::WebJoypadButton;

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
    pub fn new(canvas: OffscreenCanvas, scale: u32) -> Self {
        Self {
            inner: InnerRunner::new(canvas, scale),
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

    pub fn input_down(&mut self, key_code: &str) {
        self.inner.input_down(key_code);
    }

    pub fn input_up(&mut self, key_code: &str) {
        self.inner.input_up(key_code);
    }

    pub fn rebind_input(&mut self, id: String, button: WebJoypadButton) {
        self.inner.rebind_input(id, button);
    }

    pub fn insert_disc(&mut self, cue_content: String, bin_files: Map) {
        let mut files = std::collections::HashMap::new();
        bin_files.for_each(&mut |value, key| {
            let name = key.as_string().unwrap_or_default();
            let file: File = value.unchecked_into();
            files.insert(name, file);
        });
        self.inner.insert_disc(&cue_content, files);
    }

    pub fn save_memory_card(&mut self, port: u8) -> Vec<u8> {
        self.inner.save_memory_card(port)
    }

    pub fn load_memory_card(&mut self, port: u8, data: Vec<u8>) {
        self.inner.load_memory_card(port, &data);
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

    pub fn dump_accurate_sample(&self) -> Option<js_sys::Uint8Array> {
        self.inner
            .dump_accurate_sample()
            .map(|(_, _, bytes)| js_sys::Uint8Array::from(bytes.as_slice()))
    }

    pub fn dump_enhanced_sample(&self) -> Option<js_sys::Uint8Array> {
        self.inner
            .dump_enhanced_sample()
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

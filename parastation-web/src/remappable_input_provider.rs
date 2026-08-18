/*
 * @file /parastation-web/src/remappable_input_provider.rs
 * @brief
 * Web frontend input provider implementation for ParaStation. Generic input provider with variable mappings to allow
 * for user remapping. Exposes methods to press keys and set the state of the joypad buttons, which are called from the
 * JavaScript side via postMessage.
 *
 * -----
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use wasm_bindgen::prelude::*;

use parastation_core::sio0::{InputProvider, JoypadButton};

#[wasm_bindgen]
#[derive(Copy, Clone)]
pub enum WebJoypadButton {
    Select,
    L3,
    R3,
    Start,
    Up,
    Right,
    Down,
    Left,
    L2,
    R2,
    L1,
    R1,
    Triangle,
    Circle,
    Cross,
    Square,
}

impl From<WebJoypadButton> for JoypadButton {
    fn from(b: WebJoypadButton) -> Self {
        match b {
            WebJoypadButton::Select => JoypadButton::Select,
            WebJoypadButton::L3 => JoypadButton::L3,
            WebJoypadButton::R3 => JoypadButton::R3,
            WebJoypadButton::Start => JoypadButton::Start,
            WebJoypadButton::Up => JoypadButton::Up,
            WebJoypadButton::Right => JoypadButton::Right,
            WebJoypadButton::Down => JoypadButton::Down,
            WebJoypadButton::Left => JoypadButton::Left,
            WebJoypadButton::L2 => JoypadButton::L2,
            WebJoypadButton::R2 => JoypadButton::R2,
            WebJoypadButton::L1 => JoypadButton::L1,
            WebJoypadButton::R1 => JoypadButton::R1,
            WebJoypadButton::Triangle => JoypadButton::Triangle,
            WebJoypadButton::Circle => JoypadButton::Circle,
            WebJoypadButton::Cross => JoypadButton::Cross,
            WebJoypadButton::Square => JoypadButton::Square,
        }
    }
}

#[derive(Clone)]
pub struct JoypadState {
    bits: Arc<AtomicU16>,
}

impl JoypadState {
    pub fn new() -> Self {
        Self {
            bits: Arc::new(AtomicU16::new(0xFFFF)), // all buttons released
        }
    }

    pub fn press(&self, button: JoypadButton) {
        self.bits.fetch_and(!(button as u16), Ordering::Relaxed);
    }

    pub fn release(&self, button: JoypadButton) {
        self.bits.fetch_or(button as u16, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> u16 {
        self.bits.load(Ordering::Relaxed)
    }
}

pub struct RemappableInputProvider {
    map: HashMap<String, JoypadButton>,
    state: JoypadState,
}

impl RemappableInputProvider {
    pub fn new(state: JoypadState, initial_mapping: &[(&str, JoypadButton)]) -> Self {
        let map = initial_mapping
            .iter()
            .map(|(id, button)| (id.to_string(), *button))
            .collect();

        Self { map, state }
    }

    /// Rebinds a single input to button mapping
    pub fn rebind(&mut self, id: String, button: JoypadButton) {
        self.map.insert(id, button);
    }

    pub fn press(&self, id: &str) {
        if let Some(&button) = self.map.get(id) {
            self.state.press(button);
        }
    }

    pub fn release(&self, id: &str) {
        if let Some(&button) = self.map.get(id) {
            self.state.release(button);
        }
    }
}

impl InputProvider for RemappableInputProvider {
    fn get_joypad_state(&self) -> u16 {
        self.state.snapshot()
    }
}

// Shared handle so main thread can send messages while the PS1 core can read input
pub struct SharedInputHandle(pub Rc<RefCell<RemappableInputProvider>>);

impl SharedInputHandle {
    pub fn new(shared: Rc<RefCell<RemappableInputProvider>>) -> Self {
        Self(shared)
    }
}

impl InputProvider for SharedInputHandle {
    fn get_joypad_state(&self) -> u16 {
        self.0.borrow().get_joypad_state()
    }
}

// Default mappings
pub const DEFAULT_KEYBOARD_MAPPING: &[(&str, JoypadButton)] = &[
    ("ArrowUp", JoypadButton::Up),
    ("ArrowDown", JoypadButton::Down),
    ("ArrowLeft", JoypadButton::Left),
    ("ArrowRight", JoypadButton::Right),
    ("Enter", JoypadButton::Start),
    ("ShiftRight", JoypadButton::Select),
    ("KeyK", JoypadButton::Cross),
    ("KeyJ", JoypadButton::Square),
    ("KeyL", JoypadButton::Circle),
    ("KeyI", JoypadButton::Triangle),
    ("KeyQ", JoypadButton::L1),
    ("KeyE", JoypadButton::R1),
    ("Digit1", JoypadButton::L2),
    ("Digit3", JoypadButton::R2),
    ("KeyC", JoypadButton::L3),
    ("KeyV", JoypadButton::R3),
];

pub struct DummyInputProvider;
impl InputProvider for DummyInputProvider {
    fn get_joypad_state(&self) -> u16 {
        0xFFFF
    }
}

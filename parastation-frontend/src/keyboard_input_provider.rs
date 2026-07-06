/*
 * @file /parastation-frontend/src/keyboard_input_provider.rs
 * @brief
 * Keyboard input provider implementation for handling keyboard event with a static key mapping..
 *
 * -----
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

use parastation_core::sio0::{InputProvider, JoypadButton};
use winit::keyboard::KeyCode;

/// Static mapping from physical keyboard keys to joypad buttons
const KEY_MAP: &[(KeyCode, JoypadButton)] = &[
    (KeyCode::ArrowUp, JoypadButton::Up),
    (KeyCode::ArrowDown, JoypadButton::Down),
    (KeyCode::ArrowLeft, JoypadButton::Left),
    (KeyCode::ArrowRight, JoypadButton::Right),
    (KeyCode::Enter, JoypadButton::Start),
    (KeyCode::ShiftRight, JoypadButton::Select),
    (KeyCode::KeyK, JoypadButton::Cross),
    (KeyCode::KeyJ, JoypadButton::Square),
    (KeyCode::KeyL, JoypadButton::Circle),
    (KeyCode::KeyI, JoypadButton::Triangle),
    (KeyCode::KeyQ, JoypadButton::L1),
    (KeyCode::KeyE, JoypadButton::R1),
    (KeyCode::Digit1, JoypadButton::L2),
    (KeyCode::Digit3, JoypadButton::R2),
    (KeyCode::KeyC, JoypadButton::L3),
    (KeyCode::KeyV, JoypadButton::R3),
];

/// Looks up the JoypadButton bitmask bound to a given key, if any
fn button_for_key(key: KeyCode) -> Option<u16> {
    KEY_MAP
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, button)| *button as u16)
}

#[derive(Clone)]
pub struct KeyboardState {
    bits: Arc<AtomicU16>,
}

impl KeyboardState {
    pub fn new() -> Self {
        Self {
            bits: Arc::new(AtomicU16::new(0xFFFF)), // all buttons released
        }
    }

    pub fn key_pressed(&self, key: KeyCode) {
        if let Some(mask) = button_for_key(key) {
            self.bits.fetch_and(!mask, Ordering::Relaxed);
        }
    }

    pub fn key_released(&self, key: KeyCode) {
        if let Some(mask) = button_for_key(key) {
            self.bits.fetch_or(mask, Ordering::Relaxed);
        }
    }
}

pub struct KeyboardInputProvider {
    state: KeyboardState,
}

impl KeyboardInputProvider {
    pub fn new(state: KeyboardState) -> Self {
        Self { state }
    }
}

impl InputProvider for KeyboardInputProvider {
    fn get_joypad_state(&self) -> u16 {
        self.state.bits.load(Ordering::Relaxed)
    }
}

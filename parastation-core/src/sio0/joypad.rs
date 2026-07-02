/*
 * @file /parastation-core/src/sio0/joypad.rs
 * @brief
 * Struct and implementation for the joypad's communication sequence with the PS1, alongside a trait for the
 * frontend to implement the specific behaviour of the joypad to read user input.
 *
 * -----
 */

// Imports
use crate::sio0::sio_device::SioDevice;

/// Represents a button on the joypad, with a name and a bitmask for the button's position in the 16-bit response
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoypadButton {
    // buttons_lo
    Select = 1 << 0,
    L3 = 1 << 1,
    R3 = 1 << 2,
    Start = 1 << 3,
    Up = 1 << 4,
    Right = 1 << 5,
    Down = 1 << 6,
    Left = 1 << 7,
    // buttons_hi
    L2 = 1 << 8,
    R2 = 1 << 9,
    L1 = 1 << 10,
    R1 = 1 << 11,
    Triangle = 1 << 12,
    Circle = 1 << 13,
    Cross = 1 << 14,
    Square = 1 << 15,
}

/// Trait to be implemented by the frontend to provide the current state of the joypad buttons
pub trait InputProvider {
    /// Returns a 16 bit value representing the current state of the joypad buttons, where each bit corresponds to a button
    fn get_joypad_state(&self) -> u16;
}

// Handshake for the joypad follows
// 1. waiting for 0x01 from the PS1, respond with 0xFF
// 2. wait for 0x42 from the PS1, respond with 0x41
// 3. wait for 0x00 from the PS1, respond with 0x5A
// 4. wait for 0x00 from the PS1, respond with the first byte of the button state (buttons_lo)
// 5. wait for 0x00 from the PS1, respond with the second byte of the button state (buttons_hi)
// We can represent these with a FSM, then exchange() sends the appropriate response based on the current state and
// the input byte from the PS1
#[derive(Debug, Clone, Copy, PartialEq)]
enum JoyPadState {
    Idle,     // waiting for address byte (0x01)
    Command,  // received address, waiting for 0x42
    IdHi,     // sent 0x41, waiting for next
    Buttons1, // sent 0x5A, ready to send lo byte
    Buttons2, // sent lo byte, ready to send hi byte
}

/// Represents the joypad device, which implements the SioDevice trait and communicates with the PS1 via SIO0
pub struct Joypad {
    fsm: JoyPadState,
    input: Box<dyn InputProvider>,
    selected: bool,
}

impl Joypad {
    pub fn new(input: Box<dyn InputProvider>) -> Self {
        Self {
            fsm: JoyPadState::Idle,
            input,
            selected: false,
        }
    }
}

impl SioDevice for Joypad {
    fn exchange(&mut self, byte: u8) -> (u8, bool) {
        match self.fsm {
            JoyPadState::Idle => {
                let dsr = byte == 0x01;
                if dsr {
                    self.fsm = JoyPadState::Command;
                } else {
                    self.fsm = JoyPadState::Idle;
                }
                (0xFF, dsr)
            }
            JoyPadState::Command => {
                let dsr = byte == 0x42;
                if dsr {
                    self.fsm = JoyPadState::IdHi;
                } else {
                    self.fsm = JoyPadState::Idle;
                }
                (0x41, dsr)
            }
            JoyPadState::IdHi => {
                self.fsm = JoyPadState::Buttons1;
                (0x5A, true)
            }
            JoyPadState::Buttons1 => {
                let buttons = self.input.get_joypad_state();
                let lo = (buttons & 0xFF) as u8;
                self.fsm = JoyPadState::Buttons2;
                (lo, true)
            }
            JoyPadState::Buttons2 => {
                let buttons = self.input.get_joypad_state();
                let hi = (buttons >> 8) as u8;
                self.fsm = JoyPadState::Idle;
                (hi, false) // Last byte, so DSR false (no more bytes to send)
            }
        }
    }

    fn reset(&mut self) {
        self.fsm = JoyPadState::Idle;
    }

    fn is_selected(&self) -> bool {
        self.selected
    }

    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

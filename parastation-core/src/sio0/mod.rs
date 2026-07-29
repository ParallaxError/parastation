/*
 * @file /parastation-core/src/sio0/mod.rs
 * @brief
 * SioController implementation for the PS1, which handles communication with SIO0 devices (joypads and memory cards).
 *
 * -----
 */

mod joypad;
pub use joypad::{InputProvider, Joypad, JoypadButton};
mod memory_card;
use memory_card::MemoryCard;
mod sio_device;
use sio_device::SioDevice;

// Imports
use crate::elog;
use crate::interrupt_controller::{Interrupt, InterruptController};
use crate::scheduler::{Scheduler, SchedulerEvent};

enum ActiveDevice {
    None,
    Joypad1,
    Joypad2,
    MemoryCard1,
    MemoryCard2,
}

pub struct SioController {
    pub joypad1: Joypad,
    pub joypad2: Joypad,
    pub memory_card1: MemoryCard,
    pub memory_card2: MemoryCard,

    active_device: ActiveDevice,
    transfer_active: bool,
    port_select: bool, // false = port 1, true = port 2

    // Registers
    stat: u16,
    mode: u16,
    ctrl: u16,
    baud: u16,
    rx_data: u8,
}

impl SioController {
    pub fn new(joy1_input: Box<dyn InputProvider>, joy2_input: Box<dyn InputProvider>) -> Self {
        Self {
            joypad1: Joypad::new(joy1_input),
            joypad2: Joypad::new(joy2_input),
            memory_card1: MemoryCard::new(),
            memory_card2: MemoryCard::new(),

            active_device: ActiveDevice::None,
            transfer_active: false,
            port_select: false,

            stat: 0,
            mode: 0,
            ctrl: 0,
            baud: 0,
            rx_data: 0,
        }
    }
}

// Register interface
impl SioController {
    pub fn read_register(&mut self, offset: u32) -> u32 {
        match offset {
            0x0 => self.read_data() as u32,
            0x4 => self.read_stat(),
            0x8 => self.read_mode() as u32,
            0xA => self.read_ctrl() as u32,
            0xE => self.read_baud() as u32,
            _ => {
                elog!("SIO0: Invalid read register offset: 0x{:X}", offset);
                0
            }
        }
    }

    pub fn write_register(&mut self, offset: u32, value: u32, scheduler: &mut Scheduler) {
        match offset {
            0x0 => self.write_data(value as u8, scheduler),
            0x4 => {
                elog!("SIO0: Invalid write to STAT register");
            }
            0x8 => self.write_mode(value as u16),
            0xA => self.write_ctrl(value as u16),
            0xE => self.write_baud(value as u16),
            _ => {
                elog!("SIO0: Invalid write register offset: 0x{:X}", offset);
            }
        }
    }

    fn write_ctrl(&mut self, value: u16) {
        self.ctrl = value;
        self.port_select = (value >> 13) & 1 != 0;

        if value & (1 << 6) != 0 {
            // Reset
            self.stat = 0;
            self.mode = 0;
            self.ctrl = 0;
            self.baud = 0;
            self.joypad1.reset();
            self.joypad2.reset();
            self.memory_card1.reset();
            self.memory_card2.reset();
            self.active_device = ActiveDevice::None;
            self.transfer_active = false;
        }

        if value & (1 << 4) != 0 {
            // Ack clears STAT bits 3,4,5,9
            self.stat &= !((1 << 3) | (1 << 4) | (1 << 5) | (1 << 9));
        }

        self.transfer_active = value & (1 << 1) != 0;
        if !self.transfer_active {
            // Reset deasserted CS device
            match self.active_device {
                ActiveDevice::Joypad1 => self.joypad1.reset(),
                ActiveDevice::Joypad2 => self.joypad2.reset(),
                ActiveDevice::MemoryCard1 => self.memory_card1.reset(),
                ActiveDevice::MemoryCard2 => self.memory_card2.reset(),
                ActiveDevice::None => {}
            }
            self.active_device = ActiveDevice::None;
        }
    }

    fn write_data(&mut self, value: u8, scheduler: &mut Scheduler) {
        // Only process data if transfer is active
        if !self.transfer_active {
            return;
        }

        // First byte of a transfer, selecting a device
        if matches!(self.active_device, ActiveDevice::None) {
            self.active_device = match value {
                0x01 => {
                    if self.port_select {
                        ActiveDevice::Joypad2
                    } else {
                        ActiveDevice::Joypad1
                    }
                }
                0x81 => {
                    if self.port_select {
                        ActiveDevice::MemoryCard2
                    } else {
                        ActiveDevice::MemoryCard1
                    }
                }
                _ => ActiveDevice::None,
            };

            if matches!(self.active_device, ActiveDevice::None) {
                return; // Unrecognized address
            }
        }

        let (response, dsr) = match self.active_device {
            ActiveDevice::Joypad1 => self.joypad1.exchange(value),
            ActiveDevice::Joypad2 => self.joypad2.exchange(value),
            ActiveDevice::MemoryCard1 => self.memory_card1.exchange(value),
            ActiveDevice::MemoryCard2 => self.memory_card2.exchange(value),
            ActiveDevice::None => (0xFF, false),
        };

        let delay = (16 * self.baud as u64).max(1);
        scheduler.schedule(
            SchedulerEvent::SioResponse {
                byte: response,
                dsr,
            },
            delay,
        );

        if !dsr {
            self.active_device = ActiveDevice::None;
        }
    }

    pub fn read_data(&mut self) -> u8 {
        // Clear RX not empty (bit 1)
        self.stat &= !(1 << 1);
        self.rx_data
    }

    pub fn read_stat(&mut self) -> u32 {
        let value = (self.stat | (1 << 0) | (1 << 2)) as u32;
        // Need to clear ACK pulse as its a pulse
        self.stat &= !(1 << 7);
        value
    }

    pub fn read_ctrl(&self) -> u16 {
        self.ctrl
    }

    pub fn read_mode(&self) -> u16 {
        self.mode
    }

    pub fn write_mode(&mut self, value: u16) {
        self.mode = value;
    }

    pub fn read_baud(&self) -> u16 {
        self.baud
    }

    pub fn write_baud(&mut self, value: u16) {
        self.baud = value;
    }

    pub fn handle_event(
        &mut self,
        byte: u8,
        dsr: bool,
        interrupt_controller: &mut InterruptController,
    ) {
        self.rx_data = byte;
        self.stat |= 1 << 1; // RXRDY always, response is always deliverable
        if dsr {
            self.stat |= (1 << 7) | (1 << 9);
            interrupt_controller.raise_interrupt(Interrupt::Controller);
        }
    }
}

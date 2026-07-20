/*
 * @file /parastation-core/src/interrupt_controller.rs
 * @brief
 * Holds the relevant registers for the interrupt controller, as well as an enum for the
 * potential interrupt types
 * -----
 */

/// A hardware interrupt type, corresponding to a bit in the I_STAT and I_MASK registers of the interrupt
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interrupt {
    VBlank = 0x0,     // Vertical blank interrupt
    GPU = 0x1,        // GPU interrupt
    CDROM = 0x2,      // CD-ROM interrupt
    DMA = 0x3,        // DMA interrupt
    TMR0 = 0x4,       // Timer 0 interrupt
    TMR1 = 0x5,       // Timer 1 interrupt
    TMR2 = 0x6,       // Timer 2 interrupt
    Controller = 0x7, // Controller interrupt
    SIO = 0x8,        // Serial I/O interrupt
    SPU = 0x9,        // SPU interrupt
    PIO = 0xA,        // PIO interrupt
}

/// Interrupt controller struct, holding the relevant registers and methods for handling interrupts
pub struct InterruptController {
    stat: u16, // I_STAT: pending interrupts
    mask: u16, // I_MASK: enabled interrupts
}

impl InterruptController {
    pub fn new() -> Self {
        Self { stat: 0, mask: 0 }
    }

    pub fn read_stat(&self) -> u32 {
        self.stat as u32
    }

    pub fn read_mask(&self) -> u32 {
        self.mask as u32
    }

    pub fn write_stat(&mut self, value: u16) {
        self.stat &= value;
    }

    pub fn write_mask(&mut self, value: u16) {
        self.mask = value;
    }

    pub fn raise_interrupt(&mut self, interrupt: Interrupt) {
        self.stat |= 1 << (interrupt as u16);
    }

    pub fn pending(&self) -> bool {
        return (self.stat & self.mask) != 0;
    }
}

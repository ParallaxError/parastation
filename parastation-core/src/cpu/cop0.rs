/*
 * @file /parastation-core/src/cpu/cop0.rs
 * @brief
 * Coprocessor 0, mandated by the MIPS architecture. Handles exceptions, interrupts, traps, syscalls
 * etc on the PS1. 
 * 
 * Encapsulates the various CP0 registers and provides an interface for the CPU to interact with 
 * them.
 * 
 * -----
 */

/// Represents one of the coprocessor 0 registers.
pub struct Cop0Register {
    index: u8,
}

/// Coprocessor 0, mandated by the MIPS architecture. Handles exceptions, interrupts, traps, 
/// syscalls, etc on the PS1. 
/// 
/// Encapsulates the various CP0 registers and provides an interface for the CPU to interact with 
/// them.
pub struct Cop0 {
    sr: u32, // Status Register
    cause: u32, // Cause Register
    epc: u32, // Exception Program Counter
}

impl Cop0 {
    pub fn new() -> Self {
        Self {
            sr: 0,
            cause: 0,
            epc: 0,
        }
    }
}

// Read and write interfaces
// Section 2.35 of https://vojty.github.io/psx-guide/guide.pdf contains the register indices.
impl Cop0
{
    pub fn read(&self, reg: u8) -> u32 {
        match reg {
            12 => self.sr,
            13 => self.cause,
            14 => self.epc,
            _ => panic!("Invalid CP0 register index: {}", reg),
        }
    }

    pub fn write(&mut self, reg: u8, value: u32) {
        match reg {
            12 => self.sr = value,
            13 => self.cause = value,
            14 => self.epc = value,
            _ => panic!("Invalid CP0 register index: {}", reg),
        }
    }
}
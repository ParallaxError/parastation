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
#[derive(Debug)]
pub struct Cop0Register(pub u8);

/// Coprocessor 0, mandated by the MIPS architecture. Handles exceptions, interrupts, traps,
/// syscalls, etc on the PS1.
///
/// Encapsulates the various CP0 registers and provides an interface for the CPU to interact with
/// them.
pub struct Cop0 {
    sr: u32,       // Status Register
    cause: u32,    // Cause Register
    epc: u32,      // Exception Program Counter
    badaddrv: u32, // Bad Virtual Address, set by exceptions
    dcic: u32,     // Debug Control/Identification Register
}

impl Cop0 {
    pub fn new() -> Self {
        Self {
            sr: 0,
            cause: 0,
            epc: 0,
            badaddrv: 0,
            dcic: 0,
        }
    }
}

// Read and write interfaces
// Section 2.35 of https://vojty.github.io/psx-guide/guide.pdf contains the register indices.
impl Cop0 {
    pub fn read(&self, reg: Cop0Register) -> u32 {
        match reg {
            Cop0Register(3) | Cop0Register(5) | Cop0Register(6) | Cop0Register(9) => 0,
            Cop0Register(7) => self.dcic,
            Cop0Register(8) => self.badaddrv,
            Cop0Register(11) => 0,
            Cop0Register(12) => self.sr,
            Cop0Register(13) => self.cause,
            Cop0Register(14) => self.epc,
            Cop0Register(15) => 0x0000_0002, // Processor ID register, R3000A
            _ => {
                eprintln!("Invalid CP0 register index: {:?}", reg);
                0
            }
        }
    }

    pub fn write(&mut self, reg: Cop0Register, value: u32) {
        match reg {
            Cop0Register(3) | Cop0Register(5) | Cop0Register(6) | Cop0Register(9)
            | Cop0Register(11) => (),
            Cop0Register(7) => self.dcic = value,
            Cop0Register(8) => self.badaddrv = value,
            Cop0Register(12) => self.sr = value,
            Cop0Register(13) => self.cause = value,
            Cop0Register(14) => self.epc = value,
            _ => eprintln!("Invalid CP0 register index: {:?}", reg),
        }
    }
}

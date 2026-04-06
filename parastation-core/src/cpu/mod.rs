/*
 * @file /parastation-core/src/cpu/mod.rs
 * @brief
 * Encapsulation of the PS1 CPU, a MIPS R3000A. Contains the CPU state (registers, PC, etc.) 
 * 
 * -----
 */

mod cop0;
mod gte;
pub mod ir;

pub use cop0::Cop0;
pub use cop0::Cop0Register;
pub use gte::Gte;
pub use gte::GteRegister;

/// Represents one of the 32 GPRs of the MIPS R3000A.
#[derive(Debug)]
pub struct MipsRegister(pub u8);

/// Represents the PS1 CPU, a MIPS R3000A. Contains the CPU state (registers, PC, etc.)
pub struct Cpu {
    registers: [u32; 32], // 32 general purpose registers
    pc: u32, // Program counter
    next_pc: u32, // Next program counter (for branch delay slot)

    hi: u32, // HI register for multiplication/division results
    lo: u32, // LO register for multiplication/division results

    cop0: Cop0, // Coprocessor 0 for system control
    gte: Gte, // Geometry Transformation Engine for 3D graphics

    in_delay_slot: bool, // Whether currently executing an instruction in a branch delay slot
    load_delay: Option<(MipsRegister, u32)>, // Pending load (dst reg, value)
}

// Reset behaviour
impl Cpu {
    pub fn new() -> Self {
        Self {
            registers: [0; 32],
            pc: 0xBFC0_0000, // Start of BIOS in memory
            next_pc: 0xBFC0_0004,

            hi: 0,
            lo: 0,

            cop0: Cop0::new(),
            gte: Gte::new(),

            in_delay_slot: false,
            load_delay: None,
        }
    }
}

// Property exposing
impl Cpu {
    // Registers
    pub fn read_reg(&self, reg: MipsRegister) -> u32 {
         self.registers[reg.0 as usize] 
    }

    pub fn write_reg(&mut self, reg: MipsRegister, value: u32) {
        if reg.0 != 0 { // Register 0 is hardwired to 0
            self.registers[reg.0 as usize] = value;
        }
    }

    // PC and next PC
    pub fn pc(&self) -> u32 {
        self.pc
    }

    pub fn set_pc(&mut self, value: u32) {
        self.pc = value;
    }

    pub fn next_pc(&self) -> u32 {
        self.next_pc
    }

    pub fn set_next_pc(&mut self, value: u32) {
        self.next_pc = value;
    }

    // Hi/lo
    pub fn hi(&self) -> u32 {
        self.hi
    }

    pub fn set_hi(&mut self, value: u32) {
        self.hi = value;
    }

    pub fn lo(&self) -> u32 {
        self.lo
    }

    pub fn set_lo(&mut self, value: u32) {
        self.lo = value;
    }

    // Coprocessor 0
    pub fn read_cop0(&self, reg: Cop0Register) -> u32 {
        self.cop0.read(reg)
    }

    pub fn write_cop0(&mut self, reg: Cop0Register, value: u32) {
        self.cop0.write(reg, value);
    }

    // Load delay slot
    pub fn commit_load_delay(&mut self) {
        if let Some((reg, value)) = self.load_delay.take() {
            self.write_reg(reg, value);
        }
    }

    pub fn set_load_delay(&mut self, reg: MipsRegister, val: u32) {
        self.load_delay = Some((reg, val));
    }

    // Branch delay slot
    pub fn in_delay_slot(&self) -> bool {
        self.in_delay_slot
    }

    pub fn set_in_delay_slot(&mut self, value: bool) {
        self.in_delay_slot = value;
    }
}
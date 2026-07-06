/*
 * @file /parastation-core/src/cpu/mod.rs
 * @brief
 * Encapsulation of the PS1 CPU, a MIPS R3000A. Contains the CPU state (registers, PC, etc.)
 *
 * -----
 */

mod cop0;
mod gte;
mod gte_div;
pub mod ir;

pub use cop0::Cop0;
pub use cop0::Cop0Register;
pub use gte::Gte;
pub use gte::GteRegister;

/// Represents one of the 32 GPRs of the MIPS R3000A.
#[derive(Debug, Clone, Copy)]
pub struct MipsRegister(pub u8);

/// Represents the PS1 CPU, a MIPS R3000A. Contains the CPU state (registers, PC, etc.)
pub struct Cpu {
    registers: [u32; 32], // 32 general purpose registers
    pc: u32,              // Program counter
    next_pc: u32,         // Next program counter (for branch delay slot)
    current_pc: u32,      // Current program counter (for exceptions)

    hi: u32, // HI register for multiplication/division results
    lo: u32, // LO register for multiplication/division results

    cop0: Cop0, // Coprocessor 0 for system control
    gte: Gte,   // Geometry Transformation Engine for 3D graphics

    in_delay_slot: bool, // Whether currently executing an instruction in a branch delay slot
    load_delay: Option<(MipsRegister, u32)>, // Pending load to commit this step
    next_load_delay: Option<(MipsRegister, u32)>, // Pending load for the following step
}

// Reset behaviour
impl Cpu {
    pub fn new() -> Self {
        Self {
            registers: [0; 32],
            pc: 0xBFC0_0000, // Start of BIOS in memory
            next_pc: 0xBFC0_0004,
            current_pc: 0xBFC0_0000,

            hi: 0,
            lo: 0,

            cop0: Cop0::new(),
            gte: Gte::new(),

            in_delay_slot: false,
            load_delay: None,
            next_load_delay: None,
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
        if reg.0 != 0 {
            // Register 0 is hardwired to 0
            self.registers[reg.0 as usize] = value;
        }
    }

    pub fn read_reg_or_pending(&self, reg: MipsRegister) -> u32 {
        // check next_load_delay first, then load_delay, then register file
        if let Some((pending_reg, val)) = self.next_load_delay {
            if pending_reg.0 == reg.0 {
                return val;
            }
        }
        if let Some((pending_reg, val)) = self.load_delay {
            if pending_reg.0 == reg.0 {
                return val;
            }
        }
        self.read_reg(reg)
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

    pub fn current_pc(&self) -> u32 {
        self.current_pc
    }

    pub fn set_current_pc(&mut self, value: u32) {
        self.current_pc = value;
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

    // Coprocessor 2
    pub fn read_gte(&self, reg: GteRegister) -> u32 {
        self.gte.read_register(reg)
    }

    pub fn write_gte(&mut self, reg: GteRegister, value: u32) {
        self.gte.write_register(reg, value);
    }

    pub fn execute_gte(&mut self, command: u32) {
        self.gte.execute_command(command);
    }

    // Load delay slot
    pub fn commit_load_delay(&mut self) {
        if let Some((reg, value)) = self.load_delay.take() {
            self.write_reg(reg, value);
        }

        // Move next-step pending load into the current commit slot.
        self.load_delay = self.next_load_delay.take();
    }

    pub fn set_load_delay(&mut self, reg: MipsRegister, val: u32) {
        if reg.0 == 0 {
            return;
        }

        // If there's a pending commit for the same register, cancel it
        // (second load to same register cancels the first)
        if let Some((pending_reg, _)) = self.load_delay {
            if pending_reg.0 == reg.0 {
                self.load_delay = None;
            }
        }

        self.next_load_delay = Some((reg, val));
    }

    // Branch delay slot
    pub fn in_delay_slot(&self) -> bool {
        self.in_delay_slot
    }

    pub fn set_in_delay_slot(&mut self, value: bool) {
        self.in_delay_slot = value;
    }
}

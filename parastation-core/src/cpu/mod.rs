/*
 * @file /parastation-core/src/cpu/mod.rs
 * @brief
 * Encapsulation of the PS1 CPU, a MIPS R3000A. Contains the CPU state (registers, PC, etc.) 
 * 
 * -----
 */

/// Represents one of the 32 GPRs of the MIPS R3000A.
pub struct MipsRegister {
    index: u8,
}

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
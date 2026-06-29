/*
 * @file /parastation-core/src/cpu/ir.rs
 * @brief
 * Representations of MIPS instructions as Rust enums.
 *
 * Provides functionality to decode raw 32-bit instruction words into these enums, which may
 * be executed by a backend.
 *
 * -----
 */

use crate::cpu::MipsRegister;
use crate::cpu::cop0::Cop0Register;
use crate::cpu::gte::GteRegister;

/// Enum representing a MIPS instruction, with variants for each instruction type (R, I, J) and
/// each opcode/funct combination. This is the main IR for MIPS instructions in the emulator.   
#[derive(Debug)]
pub enum IrOp {
    // ALU MipsRegisterister (R type) instructions
    Add {
        dst: MipsRegister,
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Addu {
        dst: MipsRegister,
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Sub {
        dst: MipsRegister,
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Subu {
        dst: MipsRegister,
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    And {
        dst: MipsRegister,
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Or {
        dst: MipsRegister,
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Xor {
        dst: MipsRegister,
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Nor {
        dst: MipsRegister,
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Slt {
        dst: MipsRegister,
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Sltu {
        dst: MipsRegister,
        lhs: MipsRegister,
        rhs: MipsRegister,
    },

    // ALU immediate (I type) instructions
    Addi {
        dst: MipsRegister,
        src: MipsRegister,
        imm: i16,
    },
    Addiu {
        dst: MipsRegister,
        src: MipsRegister,
        imm: i16,
    },
    Andi {
        dst: MipsRegister,
        src: MipsRegister,
        imm: u16,
    },
    Ori {
        dst: MipsRegister,
        src: MipsRegister,
        imm: u16,
    },
    Xori {
        dst: MipsRegister,
        src: MipsRegister,
        imm: u16,
    },
    Slti {
        dst: MipsRegister,
        src: MipsRegister,
        imm: i16,
    },
    Sltiu {
        dst: MipsRegister,
        src: MipsRegister,
        imm: i16,
    },
    Lui {
        dst: MipsRegister,
        imm: u16,
    },

    // Shifts (R type) instructions
    Sll {
        dst: MipsRegister,
        src: MipsRegister,
        shamt: u8,
    },
    Srl {
        dst: MipsRegister,
        src: MipsRegister,
        shamt: u8,
    },
    Sra {
        dst: MipsRegister,
        src: MipsRegister,
        shamt: u8,
    },
    Sllv {
        dst: MipsRegister,
        src: MipsRegister,
        shift: MipsRegister,
    },
    Srlv {
        dst: MipsRegister,
        src: MipsRegister,
        shift: MipsRegister,
    },
    Srav {
        dst: MipsRegister,
        src: MipsRegister,
        shift: MipsRegister,
    },

    // Multiply/divide (R type) instructions
    Mult {
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Multu {
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Div {
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Divu {
        lhs: MipsRegister,
        rhs: MipsRegister,
    },
    Mfhi {
        dst: MipsRegister,
    },
    Mflo {
        dst: MipsRegister,
    },
    Mthi {
        src: MipsRegister,
    },
    Mtlo {
        src: MipsRegister,
    },

    // Loads and stores (I type) instructions
    Lb {
        dst: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },
    Lbu {
        dst: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },
    Lh {
        dst: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },
    Lhu {
        dst: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },
    Lw {
        dst: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },

    Sb {
        src: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },
    Sh {
        src: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },
    Sw {
        src: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },

    // Unaligned load/store
    Lwl {
        dst: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },
    Lwr {
        dst: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },
    Swl {
        src: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },
    Swr {
        src: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },

    // Branches (I type) instructions
    Beq {
        lhs: MipsRegister,
        rhs: MipsRegister,
        offset: i16,
    },
    Bne {
        lhs: MipsRegister,
        rhs: MipsRegister,
        offset: i16,
    },
    Bgtz {
        src: MipsRegister,
        offset: i16,
    },
    Blez {
        src: MipsRegister,
        offset: i16,
    },
    Bltz {
        src: MipsRegister,
        offset: i16,
    },
    Bgez {
        src: MipsRegister,
        offset: i16,
    },
    Bltzal {
        src: MipsRegister,
        offset: i16,
    },
    Bgezal {
        src: MipsRegister,
        offset: i16,
    },

    // Jumps (J type) instructions
    J {
        target: u32,
    },
    Jal {
        target: u32,
    },
    Jr {
        src: MipsRegister,
    },
    Jalr {
        dst: MipsRegister,
        src: MipsRegister,
    },

    // COP0 instructions
    Mfc0 {
        dst: MipsRegister,
        cop_reg: Cop0Register,
    },
    Mtc0 {
        src: MipsRegister,
        cop_reg: Cop0Register,
    },
    Rfe,
    Lwc0 {
        dst: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },
    Swc0 {
        src: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },

    // COP2 (GTE)
    Mfc2 {
        dst: MipsRegister,
        cop_reg: GteRegister,
    },
    Mtc2 {
        src: MipsRegister,
        cop_reg: GteRegister,
    },
    Cfc2 {
        dst: MipsRegister,
        cop_reg: GteRegister,
    },
    Ctc2 {
        src: MipsRegister,
        cop_reg: GteRegister,
    },
    Cop2 {
        command: u32,
    },
    Lwc2 {
        dst: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },
    Swc2 {
        src: MipsRegister,
        base: MipsRegister,
        offset: i16,
    },

    // System
    Syscall,
    Break,

    // Unimplemented/illegal instruction
    Unimplemented(u32),
}

// Decoding helpers
impl IrOp {
    fn op(raw: u32) -> u8 {
        ((raw >> 26) & 0x3F) as u8
    }
    fn rs(raw: u32) -> MipsRegister {
        MipsRegister((raw >> 21) as u8 & 0x1F)
    }
    fn rt(raw: u32) -> MipsRegister {
        MipsRegister((raw >> 16) as u8 & 0x1F)
    }
    fn rd(raw: u32) -> MipsRegister {
        MipsRegister((raw >> 11) as u8 & 0x1F)
    }
    fn shamt(raw: u32) -> u8 {
        ((raw >> 6) & 0x1F) as u8
    }
    fn funct(raw: u32) -> u8 {
        (raw & 0x3F) as u8
    }
    fn imm16(raw: u32) -> u16 {
        raw as u16
    }
    fn imm26(raw: u32) -> u32 {
        raw & 0x3FFFFFF
    }

    // helpers for decoding conditions and coprocessors
    fn rt_raw(raw: u32) -> u8 {
        ((raw >> 16) & 0x1F) as u8
    }
    fn rd_raw(raw: u32) -> u8 {
        ((raw >> 11) & 0x1F) as u8
    }
    fn rs_raw(raw: u32) -> u8 {
        ((raw >> 21) & 0x1F) as u8
    }
}

// Decoding logic
// https://problemkaputt.de/psx-spx.htm#cpuopcodeencoding has all the encodings, pretty much
// just listing them verbatim with a match case. Nothing special that isn't in here
impl IrOp {
    /// Decode a raw 32-bit instruction word into an IrOp enum.
    ///
    /// Encodings detailed at https://problemkaputt.de/psx-spx.htm#cpuopcodeencoding.
    pub fn decode(raw: u32) -> Self {
        match Self::op(raw) {
            // SPECIAL
            0x00 => Self::decode_special(raw),
            // BcondZ
            0x01 => Self::decode_bcondz(raw),
            // J
            0x02 => Self::J {
                target: Self::imm26(raw),
            },
            // JAL
            0x03 => Self::Jal {
                target: Self::imm26(raw),
            },
            // BEQ
            0x04 => Self::Beq {
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
                offset: Self::imm16(raw) as i16,
            },
            // BNE
            0x05 => Self::Bne {
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
                offset: Self::imm16(raw) as i16,
            },
            // BLEZ
            0x06 => Self::Blez {
                src: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // BGTZ
            0x07 => Self::Bgtz {
                src: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // ADDI
            0x08 => Self::Addi {
                dst: Self::rt(raw),
                src: Self::rs(raw),
                imm: Self::imm16(raw) as i16,
            },
            // ADDIU
            0x09 => Self::Addiu {
                dst: Self::rt(raw),
                src: Self::rs(raw),
                imm: Self::imm16(raw) as i16,
            },
            // SLTI
            0x0A => Self::Slti {
                dst: Self::rt(raw),
                src: Self::rs(raw),
                imm: Self::imm16(raw) as i16,
            },
            // SLTIU
            0x0B => Self::Sltiu {
                dst: Self::rt(raw),
                src: Self::rs(raw),
                imm: Self::imm16(raw) as i16,
            },
            // ANDI
            0x0C => Self::Andi {
                dst: Self::rt(raw),
                src: Self::rs(raw),
                imm: Self::imm16(raw),
            },
            // ORI
            0x0D => Self::Ori {
                dst: Self::rt(raw),
                src: Self::rs(raw),
                imm: Self::imm16(raw),
            },
            // XORI
            0x0E => Self::Xori {
                dst: Self::rt(raw),
                src: Self::rs(raw),
                imm: Self::imm16(raw),
            },
            // LUI
            0x0F => Self::Lui {
                dst: Self::rt(raw),
                imm: Self::imm16(raw),
            },
            // COP0
            0x10 => Self::decode_cop0(raw),
            // COP2
            0x12 => Self::decode_cop2(raw),
            // LB
            0x20 => Self::Lb {
                dst: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // LH
            0x21 => Self::Lh {
                dst: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // LWL
            0x22 => Self::Lwl {
                dst: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // LW
            0x23 => Self::Lw {
                dst: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // LWR
            0x26 => Self::Lwr {
                dst: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // LBU
            0x24 => Self::Lbu {
                dst: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // LHU
            0x25 => Self::Lhu {
                dst: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // SB
            0x28 => Self::Sb {
                src: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // SH
            0x29 => Self::Sh {
                src: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // SWL
            0x2A => Self::Swl {
                src: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // SW
            0x2B => Self::Sw {
                src: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // SWR
            0x2E => Self::Swr {
                src: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // LWC0
            0x30 => Self::Lwc0 {
                dst: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // LWC2
            0x32 => Self::Lwc2 {
                dst: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // SWC0
            0x38 => Self::Swc0 {
                src: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // SWC2
            0x3A => Self::Swc2 {
                src: Self::rt(raw),
                base: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            // Default case for unimplemented opcodes
            _ => Self::Unimplemented(raw),
        }
    }

    pub fn decode_special(raw: u32) -> Self {
        match Self::funct(raw) {
            // SLL
            0x00 => Self::Sll {
                dst: Self::rd(raw),
                src: Self::rt(raw),
                shamt: Self::shamt(raw),
            },
            // SRL
            0x02 => Self::Srl {
                dst: Self::rd(raw),
                src: Self::rt(raw),
                shamt: Self::shamt(raw),
            },
            // SRA
            0x03 => Self::Sra {
                dst: Self::rd(raw),
                src: Self::rt(raw),
                shamt: Self::shamt(raw),
            },
            // SLLV
            0x04 => Self::Sllv {
                dst: Self::rd(raw),
                src: Self::rt(raw),
                shift: Self::rs(raw),
            },
            // SRLV
            0x06 => Self::Srlv {
                dst: Self::rd(raw),
                src: Self::rt(raw),
                shift: Self::rs(raw),
            },
            // SRAV
            0x07 => Self::Srav {
                dst: Self::rd(raw),
                src: Self::rt(raw),
                shift: Self::rs(raw),
            },
            // JR
            0x08 => Self::Jr { src: Self::rs(raw) },
            // JALR
            0x09 => Self::Jalr {
                dst: Self::rd(raw),
                src: Self::rs(raw),
            },
            // SYSCALL
            0x0C => Self::Syscall,
            // BREAK
            0x0D => Self::Break,
            // MFHI
            0x10 => Self::Mfhi { dst: Self::rd(raw) },
            // MTHI
            0x11 => Self::Mthi { src: Self::rs(raw) },
            // MFLO
            0x12 => Self::Mflo { dst: Self::rd(raw) },
            // MTLO
            0x13 => Self::Mtlo { src: Self::rs(raw) },
            // MULT
            0x18 => Self::Mult {
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // MULTU
            0x19 => Self::Multu {
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // DIV
            0x1A => Self::Div {
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // DIVU
            0x1B => Self::Divu {
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // ADD
            0x20 => Self::Add {
                dst: Self::rd(raw),
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // ADDU
            0x21 => Self::Addu {
                dst: Self::rd(raw),
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // SUB
            0x22 => Self::Sub {
                dst: Self::rd(raw),
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // SUBU
            0x23 => Self::Subu {
                dst: Self::rd(raw),
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // AND
            0x24 => Self::And {
                dst: Self::rd(raw),
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // OR
            0x25 => Self::Or {
                dst: Self::rd(raw),
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // XOR
            0x26 => Self::Xor {
                dst: Self::rd(raw),
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // NOR
            0x27 => Self::Nor {
                dst: Self::rd(raw),
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // SLT
            0x2A => Self::Slt {
                dst: Self::rd(raw),
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // SLTU
            0x2B => Self::Sltu {
                dst: Self::rd(raw),
                lhs: Self::rs(raw),
                rhs: Self::rt(raw),
            },
            // Default case
            _ => Self::Unimplemented(raw),
        }
    }

    pub fn decode_bcondz(raw: u32) -> Self {
        match Self::rt_raw(raw) {
            0x00 => Self::Bltz {
                src: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            0x01 => Self::Bgez {
                src: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            0x10 => Self::Bltzal {
                src: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            0x11 => Self::Bgezal {
                src: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            rt if rt & 0x01 == 0 => Self::Bltz {
                src: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            rt if rt & 0x01 == 1 => Self::Bgez {
                src: Self::rs(raw),
                offset: Self::imm16(raw) as i16,
            },
            _ => Self::Unimplemented(raw),
        }
    }

    fn decode_cop0(raw: u32) -> Self {
        match Self::rs_raw(raw) {
            // MFC0
            0x00 => IrOp::Mfc0 {
                dst: Self::rt(raw),
                cop_reg: Cop0Register(Self::rd_raw(raw)),
            },
            // CFC0 - not on PS1 but handle gracefully
            0x02 => {
                eprintln!("CFC0 not supported on PS1");
                IrOp::Unimplemented(raw)
            }
            // MTC0
            0x04 => IrOp::Mtc0 {
                src: Self::rt(raw),
                cop_reg: Cop0Register(Self::rd_raw(raw)),
            },
            // CTC0 - not on PS1
            0x06 => {
                eprintln!("CTC0 not supported on PS1");
                IrOp::Unimplemented(raw)
            }
            // BCnF/BCnT
            0x08 => match Self::rt_raw(raw) {
                0x00 => IrOp::Unimplemented(raw), // BCnF - not used on PS1
                0x01 => IrOp::Unimplemented(raw), // BCnT - not used on PS1
                rt => {
                    eprintln!("Unknown BC0 rt {rt:#04x}");
                    IrOp::Unimplemented(raw)
                }
            },
            // COPn commands (bit 25 set)
            0x10..=0x1F => match raw & 0x3F {
                0x01 => IrOp::Unimplemented(raw), // TLBR  - not on PS1
                0x02 => IrOp::Unimplemented(raw), // TLBWI - not on PS1
                0x06 => IrOp::Unimplemented(raw), // TLBWR - not on PS1
                0x08 => IrOp::Unimplemented(raw), // TLBP  - not on PS1
                0x10 => IrOp::Rfe,
                cmd => {
                    eprintln!("Unknown COP0 command {cmd:#04x}");
                    IrOp::Unimplemented(raw)
                }
            },
            rs => {
                eprintln!("Unknown COP0 rs {rs:#04x}");
                IrOp::Unimplemented(raw)
            }
        }
    }

    fn decode_cop2(raw: u32) -> Self {
        match Self::rs_raw(raw) {
            // MFC2
            0x00 => IrOp::Mfc2 {
                dst: Self::rt(raw),
                cop_reg: GteRegister(Self::rd_raw(raw)),
            },
            // CFC2
            0x02 => IrOp::Cfc2 {
                dst: Self::rt(raw),
                cop_reg: GteRegister(Self::rd_raw(raw)),
            },
            // MTC2
            0x04 => IrOp::Mtc2 {
                src: Self::rt(raw),
                cop_reg: GteRegister(Self::rd_raw(raw)),
            },
            // CTC2
            0x06 => IrOp::Ctc2 {
                src: Self::rt(raw),
                cop_reg: GteRegister(Self::rd_raw(raw)),
            },
            // BCnF/BCnT
            0x08 => match Self::rt_raw(raw) {
                0x00 => IrOp::Unimplemented(raw), // BCnF
                0x01 => IrOp::Unimplemented(raw), // BCnT
                rt => {
                    eprintln!("Unknown BC2 rt {rt:#04x}");
                    IrOp::Unimplemented(raw)
                }
            },
            // GTE commands (bit 25 set)
            0x10..=0x1F => IrOp::Cop2 {
                command: raw & 0x1FFFFFF,
            },
            rs => {
                eprintln!("Unknown COP2 rs {rs:#04x}");
                IrOp::Unimplemented(raw)
            }
        }
    }
}

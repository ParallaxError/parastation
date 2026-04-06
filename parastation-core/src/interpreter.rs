/*
 * @file /parastation-core/src/interpreter.rs
 * @brief
 * Interpreter for the MIPS R3000A CPU of the PS1. The simplest possible backend, 
 * which fetches an instruction from memory, calls the decoder and executes the resulting IR 
 * operation, then repeats.
 * 
 * -----
 */

use crate::backend::Backend;
use crate::system_bus::SystemBus;
use crate::cpu::{Cpu, ir::IrOp, MipsRegister, Cop0Register};

pub struct Interpreter;

// Main interpreter execution functions
impl Interpreter {
    pub fn new() -> Self {
        Self
    }

    fn execute(&mut self, op: IrOp, cpu: &mut Cpu, bus: &mut SystemBus) {
        match op {
            // Load instructions
            IrOp::Lb { dst, base, offset } => self.op_lb(dst, base, offset, cpu, bus),
            IrOp::Lbu { dst, base, offset } => self.op_lbu(dst, base, offset, cpu, bus),
            IrOp::Lh { dst, base, offset } => self.op_lh(dst, base, offset, cpu, bus),
            IrOp::Lhu { dst, base, offset } => self.op_lhu(dst, base, offset, cpu, bus),
            IrOp::Lw { dst, base, offset } => self.op_lw(dst, base, offset, cpu, bus),

            // Store instructions
            IrOp::Sb { src, base, offset } => self.op_sb(src, base, offset, cpu, bus),
            IrOp::Sh { src, base, offset } => self.op_sh(src, base, offset, cpu, bus),
            IrOp::Sw { src, base, offset } => self.op_sw(src, base, offset, cpu, bus),

            // Arithmetic instructions
            IrOp::Add { dst, lhs, rhs } => self.op_add(dst, lhs, rhs, cpu),
            IrOp::Addu { dst, lhs, rhs } => self.op_addu(dst, lhs, rhs, cpu),
            IrOp::Sub { dst, lhs, rhs } => self.op_sub(dst, lhs, rhs, cpu),
            IrOp::Subu { dst, lhs, rhs } => self.op_subu(dst, lhs, rhs, cpu),
            IrOp::Addi { dst, src, imm } => self.op_addi(dst, src, imm, cpu),
            IrOp::Addiu { dst, src, imm } => self.op_addiu(dst, src, imm, cpu),
            
            // Comparison instructions
            IrOp::Slt { dst, lhs, rhs } => self.op_slt(dst, lhs, rhs, cpu),
            IrOp::Sltu { dst, lhs, rhs } => self.op_sltu(dst, lhs, rhs, cpu),
            IrOp::Slti { dst, src, imm } => self.op_slti(dst, src, imm, cpu),
            IrOp::Sltiu { dst, src, imm } => self.op_sltiu(dst, src, imm, cpu),

            // Logical instructions
            IrOp::And { dst, lhs, rhs } => self.op_and(dst, lhs, rhs, cpu),
            IrOp::Or { dst, lhs, rhs } => self.op_or(dst, lhs, rhs, cpu),
            IrOp::Xor { dst, lhs, rhs } => self.op_xor(dst, lhs, rhs, cpu),
            IrOp::Nor { dst, lhs, rhs } => self.op_nor(dst, lhs, rhs, cpu),
            IrOp::Andi { dst, src, imm } => self.op_andi(dst, src, imm, cpu),
            IrOp::Ori { dst, src, imm } => self.op_ori(dst, src, imm, cpu),
            IrOp::Xori { dst, src, imm } => self.op_xori(dst, src, imm, cpu),

            // Shifting instructions
            IrOp::Sll { dst, src, shamt } => self.op_sll(dst, src, shamt, cpu),
            IrOp::Srl { dst, src, shamt } => self.op_srl(dst, src, shamt, cpu),
            IrOp::Sra { dst, src, shamt } => self.op_sra(dst, src, shamt, cpu),
            IrOp::Sllv { dst, src, shift } => self.op_sllv(dst, src, shift, cpu),
            IrOp::Srlv { dst, src, shift } => self.op_srlv(dst, src, shift, cpu),
            IrOp::Srav { dst, src, shift } => self.op_srav(dst, src, shift, cpu),
            IrOp::Lui { dst, imm } => self.op_lui(dst, imm, cpu),

            // Multiply/divide instructions
            IrOp::Mult { lhs, rhs } => self.op_mult(lhs, rhs, cpu),
            IrOp::Multu { lhs, rhs } => self.op_multu(lhs, rhs, cpu),
            IrOp::Div { lhs, rhs } => self.op_div(lhs, rhs, cpu),
            IrOp::Divu { lhs, rhs } => self.op_divu(lhs, rhs, cpu),
            IrOp::Mfhi { dst } => self.op_mfhi(dst, cpu),
            IrOp::Mflo { dst } => self.op_mflo(dst, cpu),
            IrOp::Mthi { src } => self.op_mthi(src, cpu),
            IrOp::Mtlo { src } => self.op_mtlo(src, cpu),

            // Jumps and branches
            IrOp::Beq { lhs, rhs, offset } => self.op_beq(lhs, rhs, offset, cpu),
            IrOp::Bgtz { src, offset } => self.op_bgtz(src, offset, cpu),
            IrOp::Blez { src, offset } => self.op_blez(src, offset, cpu),
            IrOp::Bltz { src, offset } => self.op_bltz(src, offset, cpu),
            IrOp::Bgez { src, offset } => self.op_bgez(src, offset, cpu),
            IrOp::Bltzal { src, offset } => self.op_bltzal(src, offset, cpu),
            IrOp::Bgezal { src, offset } => self.op_bgezal(src, offset, cpu),
            IrOp::J { target } => self.op_j(target, cpu),
            IrOp::Jal { target } => self.op_jal(target, cpu),
            IrOp::Jr { src } => self.op_jr(src, cpu),
            IrOp::Jalr { dst, src } => self.op_jalr(dst, src, cpu),
            IrOp::Bne { lhs, rhs, offset } => self.op_bne(lhs, rhs, offset, cpu),

            // Coprocessor opcodes
            IrOp::Mfc0 { dst, cop_reg } => self.op_mfc0(dst, cop_reg, cpu),
            IrOp::Mtc0 { src, cop_reg } => self.op_mtc0(src, cop_reg, cpu),
            IrOp::Mfc2 { dst, cop_reg } => self.op_mfc2(dst, cop_reg.0, cpu),
            IrOp::Mtc2 { src, cop_reg } => self.op_mtc2(src, cop_reg.0, cpu),
            IrOp::Cop2 { command } => self.op_cop2(command, cpu),
            IrOp::Lwc0 { dst, base, offset } => self.op_lwc0(dst, base, offset, cpu, bus),
            IrOp::Lwc2 { dst, base, offset } => self.op_lwc2(dst, base, offset, cpu, bus),
            IrOp::Swc0 { src, base, offset } => self.op_swc0(src, base, offset, cpu, bus),
            IrOp::Swc2 { src, base, offset } => self.op_swc2(src, base, offset, cpu, bus),
            IrOp::Rfe => self.op_rfe(cpu),
            _ => unimplemented!("IR operation not implemented: {:?}", op)
        }
    }
}

// Backend trait implementation
impl Backend for Interpreter {
    fn step(&mut self, cpu: &mut Cpu, bus: &mut SystemBus) {
        // Commit load delay slot
        // TODO Doesn't work properly, need to add a 1 cycle delay!
        cpu.commit_load_delay();
        // Unset branch delay 
        cpu.set_in_delay_slot(false);

        // Fetch and decode
        let raw = bus.read32(cpu.pc());
        let op = IrOp::decode(raw);
        
        // Increment PC
        cpu.set_pc(cpu.next_pc()); // Update PC
        cpu.set_next_pc(cpu.next_pc().wrapping_add(4)); // Increment next PC by 4 (size of instruction)


        self.execute(op, cpu, bus);
    }

    fn run(&mut self, cpu: &mut Cpu, bus: &mut SystemBus, cycles: u64) {
        for _ in 0..cycles {
            self.step(cpu, bus);
        }
    }
}

// Load instructions
// Load instructions have to populate the delay slot instead of writing directly to the register, 
// since the PS1's CPU has a load delay of 1 instruction.
impl Interpreter {
    fn op_lb(&mut self, dst: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        let addr = cpu.read_reg(base).wrapping_add(offset as u32);
        let value = bus.read8(addr) as i8 as i32 as u32; // Sign-extend the byte
        cpu.set_load_delay(dst, value);
    }

    fn op_lbu(&mut self, dst: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        let addr = cpu.read_reg(base).wrapping_add(offset as u32);
        let value = bus.read8(addr) as u32; // Zero-extend the byte
        cpu.set_load_delay(dst, value);
    }

    fn op_lh(&mut self, dst: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        let addr = cpu.read_reg(base).wrapping_add(offset as u32);
        let value = bus.read16(addr) as i16 as i32 as u32; // Sign-extend the halfword
        cpu.set_load_delay(dst, value);
    }

    fn op_lhu(&mut self, dst: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        let addr = cpu.read_reg(base).wrapping_add(offset as u32);
        let value = bus.read16(addr) as u32; // Zero-extend the halfword
        cpu.set_load_delay(dst, value);
    }

    fn op_lw(&mut self, dst: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        let addr = cpu.read_reg(base).wrapping_add(offset as u32);
        let value = bus.read32(addr);
        cpu.set_load_delay(dst, value);
    }
}

// Store instructions
// Store instructions should be ignored if the cache is isolated, indicated by Cop0Register 12
const CACHE_ISOLATION_BIT: u32 = 0x10000;
impl Interpreter {
    fn op_sb(&mut self, src: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        // Ignore writes when cache is isolated
        if cpu.read_cop0(Cop0Register(12)) & CACHE_ISOLATION_BIT != 0 {
            return;
        }

        let addr = cpu.read_reg(base).wrapping_add(offset as u32);
        let val = (cpu.read_reg(src) & 0xFF) as u8; // Only write the lower 8 bits
        bus.write8(addr, val);
    }

    fn op_sh(&mut self, src: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        // Ignore writes when cache is isolated
        if cpu.read_cop0(Cop0Register(12)) & CACHE_ISOLATION_BIT != 0 {
            return;
        }

        let addr = cpu.read_reg(base).wrapping_add(offset as u32);
        let val = cpu.read_reg(src) & 0xFFFF; // Only write the lower 16 bits
        bus.write16(addr, val as u16);
    }

    fn op_sw(&mut self, src: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        // Ignore writes when cache is isolated
        if cpu.read_cop0(Cop0Register(12)) & CACHE_ISOLATION_BIT != 0 {
            return;
        }

        let addr = cpu.read_reg(base).wrapping_add(offset as u32);
        let val = cpu.read_reg(src);
        bus.write32(addr, val);
    }
}

// Arithmetic operations
impl Interpreter {
    fn op_add(&mut self, dst: MipsRegister, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs);
        let rhs_val = cpu.read_reg(rhs);
        let (result, overflowed) = lhs_val.overflowing_add(rhs_val);
        if overflowed {
            // TODO Handle overflow exception
            unimplemented!("Integer overflow exception not implemented");
        } else {
            cpu.write_reg(dst, result);
        }
    }

    fn op_addu(&mut self, dst: MipsRegister, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs);
        let rhs_val = cpu.read_reg(rhs);
        let result = lhs_val.wrapping_add(rhs_val);
        cpu.write_reg(dst, result);
    }

    fn op_sub(&mut self, dst: MipsRegister, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs);
        let rhs_val = cpu.read_reg(rhs);
        let (result, overflowed) = lhs_val.overflowing_sub(rhs_val);
        if overflowed {
            // TODO Handle overflow exception
            unimplemented!("Integer overflow exception not implemented");
        } else {
            cpu.write_reg(dst, result);
        }
    }

    fn op_subu(&mut self, dst: MipsRegister, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs);
        let rhs_val = cpu.read_reg(rhs);
        let result = lhs_val.wrapping_sub(rhs_val);
        cpu.write_reg(dst, result);
    }

    fn op_addi(&mut self, dst: MipsRegister, src: MipsRegister, imm: i16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src);
        let result = src_val.wrapping_add(imm as u32);
        cpu.write_reg(dst, result);
    }
    
    fn op_addiu(&mut self, dst: MipsRegister, src: MipsRegister, imm: i16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src);
        let result = src_val.wrapping_add(imm as u32);
        cpu.write_reg(dst, result);
    }
}

// Comparison operations
impl Interpreter {
    fn op_slt(&mut self, dst: MipsRegister, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs) as i32;
        let rhs_val = cpu.read_reg(rhs) as i32;
        let result = if lhs_val < rhs_val { 1 } else { 0 };
        cpu.write_reg(dst, result);
    }

    fn op_sltu(&mut self, dst: MipsRegister, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs);
        let rhs_val = cpu.read_reg(rhs);
        let result = if lhs_val < rhs_val { 1 } else { 0 };
        cpu.write_reg(dst, result);
    }

    fn op_slti(&mut self, dst: MipsRegister, src: MipsRegister, imm: i16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src) as i32;
        let imm_val = imm as i32;
        let result = if src_val < imm_val { 1 } else { 0 };
        cpu.write_reg(dst, result);
    }

    fn op_sltiu(&mut self, dst: MipsRegister, src: MipsRegister, imm: i16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src);
        let imm_val = imm as u32;
        let result = if src_val < imm_val { 1 } else { 0 };
        cpu.write_reg(dst, result);
    }
}

// Logical operations
impl Interpreter {
    fn op_and(&mut self, dst: MipsRegister, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs);
        let rhs_val = cpu.read_reg(rhs);
        let result = lhs_val & rhs_val;
        cpu.write_reg(dst, result);
    }

    fn op_or(&mut self, dst: MipsRegister, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs);
        let rhs_val = cpu.read_reg(rhs);
        let result = lhs_val | rhs_val;
        cpu.write_reg(dst, result);
    }

    fn op_xor(&mut self, dst: MipsRegister, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs);
        let rhs_val = cpu.read_reg(rhs);
        let result = lhs_val ^ rhs_val;
        cpu.write_reg(dst, result);
    }

    fn op_nor(&mut self, dst: MipsRegister, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs);
        let rhs_val = cpu.read_reg(rhs);
        let result = !(lhs_val | rhs_val);
        cpu.write_reg(dst, result);
    }

    fn op_andi(&mut self, dst: MipsRegister, src: MipsRegister, imm: u16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src);
        let result = src_val & (imm as u32);
        cpu.write_reg(dst, result);
    }

    fn op_ori(&mut self, dst: MipsRegister, src: MipsRegister, imm: u16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src);
        let result = src_val | (imm as u32);
        cpu.write_reg(dst, result);
    }

    fn op_xori(&mut self, dst: MipsRegister, src: MipsRegister, imm: u16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src);
        let result = src_val ^ (imm as u32);
        cpu.write_reg(dst, result);
    }
}

// Shifting operations
impl Interpreter {
    fn op_sllv(&mut self, dst: MipsRegister, src: MipsRegister, shift: MipsRegister, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src);
        let shift_val = (cpu.read_reg(shift) & 0x1F) as u32; // Only use the lower 5 bits of the shift amount
        let result = src_val << shift_val;
        cpu.write_reg(dst, result);
    }

    fn op_srlv(&mut self, dst: MipsRegister, src: MipsRegister, shift: MipsRegister, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src);
        let shift_val = (cpu.read_reg(shift) & 0x1F) as u32; // Only use the lower 5 bits of the shift amount
        let result = src_val >> shift_val; // Logical right shift
        cpu.write_reg(dst, result);
    }

    fn op_srav(&mut self, dst: MipsRegister, src: MipsRegister, shift: MipsRegister, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src) as i32;
        let shift_val = (cpu.read_reg(shift) & 0x1F) as u32; // Only use the lower 5 bits of the shift amount
        let result = (src_val >> shift_val) as u32; // Arithmetic right shift
        cpu.write_reg(dst, result);
    }

    fn op_sll(&mut self, dst: MipsRegister, src: MipsRegister, shamt: u8, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src);
        let result = src_val << shamt;
        cpu.write_reg(dst, result);
    }

    fn op_srl(&mut self, dst: MipsRegister, src: MipsRegister, shamt: u8, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src);
        let result = src_val >> shamt; // Logical right shift
        cpu.write_reg(dst, result);
    }

    fn op_sra(&mut self, dst: MipsRegister, src: MipsRegister, shamt: u8, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src) as i32;
        let result = (src_val >> shamt) as u32; // Arithmetic right shift
        cpu.write_reg(dst, result);
    }

    fn op_lui(&mut self, dst: crate::cpu::MipsRegister, imm: u16, cpu: &mut Cpu) {
        cpu.write_reg(dst, (imm as u32) << 16);
    }
}

// Multiply/divide
impl Interpreter {
    fn op_mult(&mut self, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs) as i32;
        let rhs_val = cpu.read_reg(rhs) as i32;
        let result = (lhs_val as i64).wrapping_mul(rhs_val as i64);
        cpu.set_hi((result >> 32) as u32);
        cpu.set_lo(result as u32);
    }

    fn op_multu(&mut self, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs) as u64;
        let rhs_val = cpu.read_reg(rhs) as u64;
        let result = lhs_val.wrapping_mul(rhs_val);
        cpu.set_hi((result >> 32) as u32);
        cpu.set_lo(result as u32);
    }

    fn op_div(&mut self, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs) as i32;
        let rhs_val = cpu.read_reg(rhs) as i32;
        if rhs_val == 0 {
            // Division by zero behavior is undefined, but on the PS1 it sets hi to the LHS and
            // lo to FFFF_FFFF
            cpu.set_hi(lhs_val as u32);
            cpu.set_lo(0xFFFF_FFFF);
        } else {
            let quotient = lhs_val.wrapping_div(rhs_val);
            let remainder = lhs_val.wrapping_rem(rhs_val);
            cpu.set_hi(remainder as u32);
            cpu.set_lo(quotient as u32);
        }
    }

    fn op_divu(&mut self, lhs: MipsRegister, rhs: MipsRegister, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs) as u32;
        let rhs_val = cpu.read_reg(rhs) as u32;
        if rhs_val == 0 {
            // If rs is 0..+7FFFFFFFh, Lo is set to -1. Otherwise, set to +1
            cpu.set_hi(lhs_val);

            if lhs_val <= 0x7FFF_FFFF {
                cpu.set_lo(0xFFFF_FFFF);
            } else {
                cpu.set_lo(0x0000_0001);
            }
        } else if (rhs_val == 0x7FFF_FFFF) {
            // If rhs is -1, the Hi is set to 0 and Lo to -80000000h
            cpu.set_hi(0);
            cpu.set_lo(0x8000_0000);
        } else {
            let quotient = lhs_val.wrapping_div(rhs_val);
            let remainder = lhs_val.wrapping_rem(rhs_val);
            cpu.set_hi(remainder);
            cpu.set_lo(quotient);
        }
    }

    fn op_mfhi(&mut self, dst: MipsRegister, cpu: &mut Cpu) {
        cpu.write_reg(dst, cpu.hi());
    }

    fn op_mflo(&mut self, dst: MipsRegister, cpu: &mut Cpu) {
        cpu.write_reg(dst, cpu.lo());
    }

    fn op_mthi(&mut self, src: MipsRegister, cpu: &mut Cpu) {
        cpu.set_hi(cpu.read_reg(src));
    }

    fn op_mtlo(&mut self, src: MipsRegister, cpu: &mut Cpu) {
        cpu.set_lo(cpu.read_reg(src));
    }
}

// Jumps and branches
impl Interpreter {
    fn op_j(&mut self, target: u32, cpu: &mut Cpu) {
        let pc_hi = cpu.pc() & 0xF0000000;
        let target_addr = (target << 2) | pc_hi;
        cpu.set_next_pc(target_addr);
        cpu.set_in_delay_slot(true);
    }

    fn op_jal(&mut self, target: u32, cpu: &mut Cpu) {
        let pc_hi = cpu.pc() & 0xF0000000;
        let target_addr = (target << 2) | pc_hi;
        cpu.write_reg(MipsRegister(31), cpu.next_pc() + 4); // Store return address in $ra
        cpu.set_next_pc(target_addr);
        cpu.set_in_delay_slot(true);
    }

    fn op_jr(&mut self, target: MipsRegister, cpu: &mut Cpu) {
        let target_addr = cpu.read_reg(target);
        cpu.set_next_pc(target_addr);
        cpu.set_in_delay_slot(true);
    }

    // call rs,ret=rd   jalr (rd,)rs(,rd)  pc=rs, rd=$+8 ;see caution
    fn op_jalr(&mut self, dst: MipsRegister, src: MipsRegister, cpu: &mut Cpu) {
        let target_addr = cpu.read_reg(src);
        cpu.write_reg(dst, cpu.next_pc() + 4); // Store return address in rd
        cpu.set_next_pc(target_addr);
        cpu.set_in_delay_slot(true);
    }

    fn op_beq(&mut self, lhs: MipsRegister, rhs: MipsRegister, offset: i16, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs);
        let rhs_val = cpu.read_reg(rhs);
        if lhs_val == rhs_val {
            let target_pc = cpu.next_pc().wrapping_add((offset as i32 as u32) << 2);
            cpu.set_next_pc(target_pc);
            cpu.set_in_delay_slot(true);
        }
    }

    fn op_bne(&mut self, lhs: MipsRegister, rhs: MipsRegister, offset: i16, cpu: &mut Cpu) {
        let lhs_val = cpu.read_reg(lhs);
        let rhs_val = cpu.read_reg(rhs);
        if lhs_val != rhs_val {
            let target_pc = cpu.next_pc().wrapping_add((offset as i32 as u32) << 2);
            cpu.set_next_pc(target_pc);
            cpu.set_in_delay_slot(true);
        }
    }

    fn op_bltz(&mut self, src: MipsRegister, offset: i16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src) as i32;
        if src_val < 0 {
            let target_pc = cpu.next_pc().wrapping_add((offset as i32 as u32) << 2);
            cpu.set_next_pc(target_pc);
            cpu.set_in_delay_slot(true);
        }
    }

    fn op_bgez(&mut self, src: MipsRegister, offset: i16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src) as i32;
        if src_val >= 0 {
            let target_pc = cpu.next_pc().wrapping_add((offset as i32 as u32) << 2);
            cpu.set_next_pc(target_pc);
            cpu.set_in_delay_slot(true);
        }
    }

    fn op_bgtz(&mut self, src: MipsRegister, offset: i16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src) as i32;
        if src_val > 0 {
            let target_pc = cpu.next_pc().wrapping_add((offset as i32 as u32) << 2);
            cpu.set_next_pc(target_pc);
            cpu.set_in_delay_slot(true);
        }
    }

    fn op_blez(&mut self, src: MipsRegister, offset: i16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src) as i32;
        if src_val <= 0 {
            let target_pc = cpu.next_pc().wrapping_add((offset as i32 as u32) << 2);
            cpu.set_next_pc(target_pc);
            cpu.set_in_delay_slot(true);
        }
    }

    fn op_bltzal(&mut self, src: MipsRegister, offset: i16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src) as i32;
        if src_val < 0 {
            let target_pc = cpu.next_pc().wrapping_add((offset as i32 as u32) << 2);
            cpu.write_reg(MipsRegister(31), cpu.next_pc() + 4); // Store return address in $ra
            cpu.set_next_pc(target_pc);
            cpu.set_in_delay_slot(true);
        }
    }

    fn op_bgezal(&mut self, src: MipsRegister, offset: i16, cpu: &mut Cpu) {
        let src_val = cpu.read_reg(src) as i32;
        if src_val >= 0 {
            let target_pc = cpu.next_pc().wrapping_add((offset as i32 as u32) << 2);
            cpu.write_reg(MipsRegister(31), cpu.next_pc() + 4); // Store return address in $ra
            cpu.set_next_pc(target_pc);
            cpu.set_in_delay_slot(true);
        }
    }
}

// Coprocessor operations
impl Interpreter {
    fn op_mfc0(&mut self, dst: MipsRegister, cop_reg: Cop0Register, cpu: &mut Cpu) {
        let value = cpu.read_cop0(cop_reg);
        cpu.write_reg(dst, value);
    }

    fn op_mfc2(&mut self, dst: MipsRegister, cop_reg: u8, cpu: &mut Cpu) {
        unimplemented!("Coprocessor 2 is not implemented");
    }

    fn op_mtc0(&mut self, src: MipsRegister, cop_reg: Cop0Register, cpu: &mut Cpu) {
        let value = cpu.read_reg(src);
        cpu.write_cop0(cop_reg, value);
    }

    fn op_mtc2(&mut self, src: MipsRegister, cop_reg: u8, cpu: &mut Cpu) {
        unimplemented!("Coprocessor 2 is not implemented");
    }

    fn op_cop2(&mut self, command: u32, cpu: &mut Cpu) {
        unimplemented!("Coprocessor 2 is not implemented");
    }

    fn op_lwc0(&mut self, dst: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        unimplemented!("Coprocessor 0 load instructions are not implemented");
    }

    fn op_lwc2(&mut self, dst: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        unimplemented!("Coprocessor 2 load instructions are not implemented");
    }

    fn op_swc0(&mut self, src: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        unimplemented!("Coprocessor 0 store instructions are not implemented");
    }

    fn op_swc2(&mut self, src: MipsRegister, base: MipsRegister, offset: i16, cpu: &mut Cpu, bus: &mut SystemBus) {
        unimplemented!("Coprocessor 2 store instructions are not implemented");
    }

    fn op_rfe(&mut self, cpu: &mut Cpu) {
        unimplemented!("RFE instruction is not implemented");
    }
}
/*
 * @file /parastation-core/src/lib.rs
 * @brief
 * PS1 emulator core library. Exposes the top-level PS1 struct, which encapsulates all of the 
 * PS1s functionality and can be instantiated by the frontend.
 * 
 * -----
 */

#![allow(dead_code)]

// Modules
mod scratchpad;
pub mod bios;
mod ram;
pub mod gpu;
mod memory_map;
mod system_bus;
mod backend;
mod cpu;
mod interpreter;

pub use backend::Backend;
pub use interpreter::Interpreter;
pub use system_bus::SystemBus;
pub use bios::Bios;
pub use cpu::{Cpu, MipsRegister};
pub use gpu::GpuBackend;

/// Top-level PS1 struct, encapsulating the entire emulator state (CPU, memory, etc.)
pub struct Ps1<B: Backend> {
    cpu: Cpu,
    bus: SystemBus,
    backend: B,
}

impl<B: Backend> Ps1<B> {
    pub fn new(bios: Bios, instruction_backend: B, gpu_backend: Box<dyn GpuBackend>) -> Self {
        Self {
            cpu: Cpu::new(),
            bus: SystemBus::new(bios, gpu_backend),
            backend: instruction_backend,
        }
    }

    /// Load a PS-EXE file into memory at the appropriate location.
    /// https://problemkaputt.de/psxspx-cdrom-file-playstation-exe-and-system-cnf.htm
    pub fn load_exe(&mut self, exe_data: &[u8]) {
        // Verify magic "PS-x EXE"
        if &exe_data[0..8] != b"PS-x EXE" && &exe_data[0..8] != b"PS-X EXE" {
            panic!("Invalid PS-EXE file: missing magic header");
        }

        let pc           = u32::from_le_bytes(exe_data[0x10..0x14].try_into().unwrap());
        let gp           = u32::from_le_bytes(exe_data[0x14..0x18].try_into().unwrap());
        let load_addr    = u32::from_le_bytes(exe_data[0x18..0x1C].try_into().unwrap());
        let file_size    = u32::from_le_bytes(exe_data[0x1C..0x20].try_into().unwrap());
        let memfill_addr = u32::from_le_bytes(exe_data[0x28..0x2C].try_into().unwrap());
        let memfill_size = u32::from_le_bytes(exe_data[0x2C..0x30].try_into().unwrap());
        let sp_base      = u32::from_le_bytes(exe_data[0x30..0x34].try_into().unwrap());
        let sp_offset    = u32::from_le_bytes(exe_data[0x34..0x38].try_into().unwrap());

        // memfill zeroes a region of RAM before loading
        if memfill_size != 0 {
            for i in 0..memfill_size {
                self.bus.write8(memfill_addr + i, 0);
            }
        }

        // Copy code/data into RAM
        for i in 0..file_size as usize {
            self.bus.write8(load_addr + i as u32, exe_data[0x800 + i]);
        }

        // Set up CPU state
        self.cpu.set_pc(pc);
        self.cpu.set_next_pc(pc.wrapping_add(4));
        self.cpu.write_reg(MipsRegister(28), gp);

        if sp_base != 0 {
            let sp = sp_base.wrapping_add(sp_offset);
            self.cpu.write_reg(MipsRegister(29), sp); // SP
            self.cpu.write_reg(MipsRegister(30), sp); // FP = SP
        }
    }

    /// Run the emulator for a given number of cycles.
    pub fn run(&mut self, cycles: u64) {
        self.backend.run(&mut self.cpu, &mut self.bus, cycles);
    }

    /// Run the emulator until the PC reaches a specific value. Useful for running until the end of
    /// the BIOS, for example.
    pub fn run_until_pc(&mut self, target_pc: u32) {
        loop {
            self.backend.step(&mut self.cpu, &mut self.bus);
            if self.cpu.pc() == target_pc {
                break;
            }
        }
    }
}
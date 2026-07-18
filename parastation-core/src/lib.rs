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
mod backend;
pub mod bios;
mod cd_rom;
mod cpu;
mod dma;
pub mod gpu;
mod interpreter;
mod interrupt_controller;
mod memory_map;
mod ram;
mod scheduler;
mod scratchpad;
pub mod sio0;
pub mod spu;
mod system_bus;
mod timers;
mod xadpcm;

pub use backend::Backend;
pub use bios::Bios;
use cpu::{Cpu, MipsRegister};
pub use gpu::GpuBackend;
pub use interpreter::Interpreter;
pub use sio0::InputProvider;
pub use spu::SpuBackend;
use system_bus::SystemBus;

const VBLANK_CYCLES: u64 = 564480; // Number of cycles between VBlank interrupts
const SPU_CYCLES: u64 = 768; // Number of cycles between SPU ticks (44.1kHz)

/// Top-level PS1 struct, encapsulating the entire emulator state (CPU, memory, etc.)
pub struct Ps1<B: Backend> {
    cpu: Cpu,
    bus: SystemBus,
    backend: B,
}

impl<B: Backend> Ps1<B> {
    pub fn new(
        bios: Bios,
        instruction_backend: B,
        gpu_backend: Box<dyn GpuBackend>,
        spu_backend: Box<dyn SpuBackend>,
        joy1: Box<dyn InputProvider>,
        joy2: Box<dyn InputProvider>,
    ) -> Self {
        Self {
            cpu: Cpu::new(),
            bus: SystemBus::new(bios, gpu_backend, spu_backend, joy1, joy2),
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

        let pc = u32::from_le_bytes(exe_data[0x10..0x14].try_into().unwrap());
        let gp = u32::from_le_bytes(exe_data[0x14..0x18].try_into().unwrap());
        let load_addr = u32::from_le_bytes(exe_data[0x18..0x1C].try_into().unwrap());
        let file_size = u32::from_le_bytes(exe_data[0x1C..0x20].try_into().unwrap());
        let memfill_addr = u32::from_le_bytes(exe_data[0x28..0x2C].try_into().unwrap());
        let memfill_size = u32::from_le_bytes(exe_data[0x2C..0x30].try_into().unwrap());
        let sp_base = u32::from_le_bytes(exe_data[0x30..0x34].try_into().unwrap());
        let sp_offset = u32::from_le_bytes(exe_data[0x34..0x38].try_into().unwrap());

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

    fn step(&mut self) {
        self.backend.step(&mut self.cpu, &mut self.bus);
    }

    /// Run the emulator for a given number of cycles.
    pub fn run(&mut self, cycles: u64) {
        let mut remaining = cycles;
        while remaining > 0 {
            self.bus.tick(2);
            self.step();
            remaining = remaining.saturating_sub(2);
        }
    }

    /// Run the emulator until the PC reaches a specific value. Useful for running until the end of
    /// the BIOS, for example.
    pub fn run_until_pc(&mut self, target_pc: u32) {
        loop {
            self.step();
            if self.cpu.pc() == target_pc {
                break;
            }
        }
    }

    /// Display the current framebuffer.
    pub fn display(&mut self) {
        self.bus.gpu.display();
    }

    /// Insert a disc into the CD-ROM drive from the provided .cue file path.
    pub fn insert_cdrom_disc(&mut self, path: &str) {
        self.bus.insert_cdrom_disc(path);
    }
}

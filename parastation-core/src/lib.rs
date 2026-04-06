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
pub mod bios;
mod backend;
mod cpu;
mod interpreter;
mod memory_map;
mod ram;
mod system_bus;

pub use backend::Backend;
pub use interpreter::Interpreter;
pub use system_bus::SystemBus;
pub use bios::Bios;
pub use cpu::Cpu;

pub struct Ps1<B: Backend> {
    cpu: Cpu,
    bus: SystemBus,
    backend: B,
}

impl<B: Backend> Ps1<B> {
    pub fn new(bios: Bios, backend: B) -> Self {
        Self {
            cpu: Cpu::new(),
            bus: SystemBus::new(bios),
            backend,
        }
    }

    pub fn run(&mut self, cycles: u64) {
        self.backend.run(&mut self.cpu, &mut self.bus, cycles);
    }
}
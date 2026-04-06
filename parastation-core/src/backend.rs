/*
 * @file /parastation-core/src/backend.rs
 * @brief
 * Trait to define a MIPS execution backend. An interpreter is the simplest implementation,
 * then later a JIT can be implemented.
 * 
 * A backend gets a mutable reference to the CPU and system bus, and can then start executing
 * the program in memory.
 * 
 * -----
 */

use crate::cpu::Cpu;
use crate::system_bus::SystemBus;

pub trait Backend {
    fn step(&mut self, cpu: &mut Cpu, bus: &mut SystemBus);
    fn run(&mut self, cpu: &mut Cpu, bus: &mut SystemBus, cycles: u64);
}
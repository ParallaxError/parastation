/*
 * @file /parastation-core/src/system_bus.rs
 * @brief
 * Main pathway for all memory flow through the PS1 system, and owner of all hardware components. 
 * All memory reads and writes from the CPU go through the system bus, which routes them to the 
 * correct memory-mapped region/peripheral.
 * 
 * Also handles DMA transfers between peripherals, but the DMA controller itself actually builds
 * the transfers.
 * 
 * -----
 */

// Imports
use crate::bios::Bios;
use crate::ram::Ram;

/// Main pathway for all memory flow through the PS1 system, and owner of all hardware components. 
/// All memory reads and writes from the CPU go through the system bus, which routes them to the
/// correct memory-mapped region/peripheral. Also handles DMA transfers between peripherals, 
/// but the DMA controller itself actually builds the transfers.   
/// 
/// For the CPU, just exposes read and write interfaces for memory access.
pub struct SystemBus {
    // Owned memories
    bios: Bios,
    ram: Ram,
}

impl SystemBus {
    pub fn new(bios: Bios) -> Self {
        Self {
            ram: MainRam::new(),
            bios,
        }
    }
}

// Region masking
// https://github.com/simias/psx-guide, section 2.38
// The MIPS architecture has some memory regions for caching and MMU stuff (virtual memory)
// The PS1 doesn't use these features, so we just need to mask the addresses to get the correct 
// region.
// More or less verbatim the rust code from the guide

const REGION_MASK: [u32; 8] = [
    // KUSEG: 2048 MB
    0xFFFF_FFFF, 0xFFFF_FFFF, 0xFFFF_FFFF, 0xFFFF_FFFF,
    // KSEG0: 512 MB
    0x7FFF_FFFF,
    // KSEG1: 512 MB
    0x1FFF_FFFF,
    // KSEG2: 1024 MB
    0xFFFF_FFFF, 0xFFFF_FFFF,
];

/// Mask an address to get the correct region for memory access. The PS1 has 8 regions, 
/// each with a different mask. Since there is no virtual memory, the function simply removes the
/// region bits from the address to get the correct physical address for memory access.
pub fn mask_region(addr: u32) -> u32 {
    let region = (addr >> 29) as usize; // Get the top 3 bits to determine the region
    addr & REGION_MASK[region] // Mask the address to get the correct physical address
}

// Memory access
// First, a macro to simplify the repetition

macro_rules! bus_read {
    ($self:expr, $addr:expr, $method:ident) => {{
        let addr = mask_region($addr);

        if let Some(offset) = RAM.contains(addr) {
            return $self.ram.$method(offset);
        }
        if let Some(offset) = BIOS.contains(addr) {
            return $self.bios.$method(offset);
        }

        eprintln!("Unhandled read at {addr:#010x}");
        Default::default()
    }}
}

macro_rules! bus_write {
    ($self:expr, $addr:expr, $value:expr, $method:ident) => {{
        let addr = mask_region($addr);

        if let Some(offset) = RAM.contains(addr) {
            return $self.ram.$method(offset, $value);
        }

        eprintln!("Unhandled write at {addr:#010x} with value {value:#010x}");
    }}
}

impl SystemBus {
    pub fn read8 (&self, addr: u32) -> u8 { bus_read!(self, addr, read8) }
    pub fn read16(&self, addr: u32) -> u16 { bus_read!(self, addr, read16) }
    pub fn read32(&self, addr: u32) -> u32 { bus_read!(self, addr, read32) }

    pub fn write8 (&mut self, addr: u32, value: u8) { bus_write!(self, addr, value, write8) }
    pub fn write16(&mut self, addr: u32, value: u16) { bus_write!(self, addr, value, write16) }
    pub fn write32(&mut self, addr: u32, value: u32) { bus_write!(self, addr, value, write32) }
}
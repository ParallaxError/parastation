/*
 * @file /parastation-core/src/memory_map.rs
 * @brief
 * Constants for the PS1 memory map, describing the start and end address of memory mapped sections.
 * 
 * Memory mapping is done by the SystemBus, which should translate a generic address to a physical
 * address for the desired memory.
 * 
 * -----
 */

// Neat pattern from section 2.8 of https://vojty.github.io/psx-guide/guide.pdf
// The Range struct encapsulates the range of a memory mapped region and also returns the
// physical address by subtracting the base
pub struct Range {
    start: u32,
    end: u32,
}

impl Range {
    /// Check if an address is within the range, and if so return the physical address by 
    /// subtracting the base. Otherwise, return None.
    #[inline(always)]
    pub fn contains(&self, addr: u32) -> Option<u32> {
        if addr >= self.start && addr < self.end {
            Some(addr - self.start)
        } else {
            None
        }
    }
}

// Memory map constants
// More or less from the guide since it exhaustively details the PS1 memory map

// 2 MB of RAM, mirrored every 2 MB until 0x1FFF_FFFF
pub const RAM: Range = Range { start: 0x0000_0000, end: 0x0020_0000 };

// Expansion region 1 (ROM/RAM), 8 MB
pub const EXP1: Range = Range { start: 0x1F00_0000, end: 0x1F80_0000 };

// IO ports (8Kb), mapped at 0x1F00_0000
// Scratchpad (data cache used as fast RAM), 1 KB
pub const SCRATCHPAD: Range = Range { start: 0x1F80_0000, end: 0x1F80_0400 };
pub const MEMORY_CONTROL_1: Range = Range { start: 0x1F80_1000, end: 0x1F80_1024 };
pub const INTERRUPT_CONTROL: Range = Range { start: 0x1F80_1070, end: 0x1F80_1078 };
pub const DMA_REGISTERS: Range = Range { start: 0x1F80_1080, end: 0x1F80_1100 };
pub const TIMERS: Range = Range { start: 0x1F80_1100, end: 0x1F80_112C };
pub const GPU_REGISTERS: Range = Range { start: 0x1F80_1810, end: 0x1F80_1818 };
pub const SPU_REGISTERS: Range = Range { start: 0x1F80_1C00, end: 0x1F80_1E80 };
pub const IO_PORTS: Range = Range { start: 0x1F80_1000, end: 0x1F80_2000 };

// Expansion region 2: Contains serial port
pub const EXP2: Range = Range { start: 0x1F80_2000, end: 0x1F80_2080 };

// Expansion region 3, whatever purpose: 2MB at 0x1FA0_0000
pub const EXP3: Range = Range { start: 0x1FA0_0000, end: 0x1FC0_0000 };

// 512 KB of BIOS, mapped at 0x1FC0_0000
pub const BIOS: Range = Range { start: 0x1FC0_0000, end: 0x1FC8_0000 };
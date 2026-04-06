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
struct Range {
    start: u32,
    end: u32,
}

impl Range {
    /// Check if an address is within the range, and if so return the physical address by 
    /// subtracting the base. Otherwise, return None.
    #[inline(always)]
    pub fn contains(&self, addr: u32) -> Option<u32> {
        if addr >= self.start && addr <= self.end {
            Some(addr - self.start)
        } else {
            None
        }
    }
}

// Memory map constants
// More or less from the guide since it exhaustively details the PS1 memory map

// 2 MB of RAM, mirrored every 2 MB until 0x1FFF_FFFF
pub const RAM: Range = Range { start: 0x0000_0000, end: 0x1FFF_FFFF };

// 512 KB of BIOS, mapped at 0x1FC0_0000
pub const BIOS: Range = Range { start: 0x1FC0_0000, end: 0x1FC7_FFFF };
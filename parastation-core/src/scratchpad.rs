/*
 * @file /parastation-core/src/scratchpad.rs
 * @brief
 * Data cache used as fast RAM, 1KB mapped at 0x1F80_0000.
 * 
 * -----
 */

/// Data cache used as fast RAM, 1KB mapped at 0x1F80_0000. Exposes functionality for reads and 
/// writes to scratchpad memory. The scratchpad is a small, fast memory region used for 
/// temporary storage by the BIOS.
pub struct Scratchpad {
    data: Box<[u8]>, // 1KB of scratchpad memory
}

const SCRATCHPAD_SIZE: usize = 1024;

impl Scratchpad {
    pub fn new() -> Self {
        Scratchpad {
            data: vec![0u8; SCRATCHPAD_SIZE].into_boxed_slice(),
        }
    }

    // Memory access
    pub fn read8(&self, offset: u32) -> u8 {
        self.data[offset as usize]
    }

    pub fn read16(&self, offset: u32) -> u16 {
        let o = offset as usize;
        u16::from_le_bytes(self.data[o..o + 2].try_into().unwrap())
    }

    pub fn read32(&self, offset: u32) -> u32 {
        let o = offset as usize;
        u32::from_le_bytes(self.data[o..o + 4].try_into().unwrap())
    }

    pub fn write8(&mut self, offset: u32, value: u8) {
        self.data[offset as usize] = value;
    }

    pub fn write16(&mut self, offset: u32, value: u16) {
        let o = offset as usize;
        self.data[o..o + 2].copy_from_slice(&value.to_le_bytes());
    }

    pub fn write32(&mut self, offset: u32, value: u32) {
        let o = offset as usize;
        self.data[o..o + 4].copy_from_slice(&value.to_le_bytes());
    }
}
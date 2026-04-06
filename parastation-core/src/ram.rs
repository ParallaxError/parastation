/*
 * @file /parastation-core/src/ram.rs
 * @brief
 * Main PS1 RAM. Provides read and write access to the 2MB of RAM available on the PS1, mapped at 
 * physical address 0x00000000.
 * 
 * -----
 */

/// Main PS1 RAM. Provides read and write access to the 2MB of RAM available on the PS1, mapped at
/// physical address 0x00000000. Exposes functionality for reads and writes to RAM memory.
/// 
/// The RAM is mirrored thrice at 0x00200000, 0x00400000, and 0x00600000, it is the responsibility
/// of the system bus to remove the higher bits of the address to ensure the correct RAM location is
/// accessed.
struct Ram {
    data: Box<[u8; 2 * 1024 * 1024]>, // PS1 RAM is 2MB
}

impl Ram {
    pub fn new() -> Self {
        Ram {
            data: Box::new([0; 2 * 1024 * 1024]),
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
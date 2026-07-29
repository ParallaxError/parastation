/*
 * @file /parastation-core/src/bios.rs
 * @brief
 * Read-only Memory (ROM) for the PS1 BIOS. Can load initial BIOS data from a file, and provides an
 * interface to read from but not write to BIOS memory.
 *
 * -----
 */

/// ROM image of the PS1 BIOS. Provides read-only access to the BIOS data.
///
/// Mapped at physical address 0x1FC00000, so read and writes have this address as a base.
/// Exposes functionality for reads and to load BIOS data from a file.
pub struct Bios {
    data: Box<[u8]>, // PS1 BIOS is 512KB
}

const BIOS_SIZE: usize = 512 * 1024;

impl Bios {
    pub fn new(bios_data: Box<[u8]>) -> Self {
        assert_eq!(bios_data.len(), BIOS_SIZE, "BIOS data must be 512KB");
        Self { data: bios_data }
    }
}

// Memory access
impl Bios {
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
}

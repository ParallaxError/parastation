/*
 * @file /parastation-core/src/bios.rs
 * @brief
 * Read-only Memory (ROM) for the PS1 BIOS. Can load initial BIOS data from a file, and provides an
 * interface to read from but not write to BIOS memory.
 * 
 * -----
 */

// Imports
use std::fs::File;
use std::io::{Read, Result};
use std::path::Path;

/// ROM image of the PS1 BIOS. Provides read-only access to the BIOS data.
/// 
/// Mapped at physical address 0x1FC00000, so read and writes have this address as a base.
/// Exposes functionality for reads and to load BIOS data from a file.
pub struct Bios {
    data: Box<[u8; 512 * 1024]>, // PS1 BIOS is 512KB
}

// File IO
impl Bios {
    /// Load BIOS data from a file at the given path. The file must be exactly 512KB in size.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let data = fs::read(path)
            .map_err(|e| format!("Failed to read BIOS file: {}", e))?;

        if data.len() != 512 * 1024 {
            return Err(format!("Invalid BIOS file size: expected 512KB, got {} bytes", data.len()));
        }

        let mut buf = Box::new([0u8; 512 * 1024]);
        buf.copy_from_slice(&data);

        Ok(Bios { data: buf })
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
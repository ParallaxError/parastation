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
use crate::cd_rom::CdRom;
use crate::dma::{DmaController, DmaTransfer};
use crate::gpu::{Gpu, GpuBackend};
use crate::interrupt_controller::InterruptController;
use crate::memory_map::*;
use crate::ram::Ram;
use crate::scratchpad::Scratchpad;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
enum AccessWidth {
    Byte,
    Half,
    Word,
}

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
    scratchpad: Scratchpad,
    pub interrupt_controller: InterruptController,
    dma: DmaController,
    pub gpu: Gpu,
    cd_rom: CdRom,
}

impl SystemBus {
    pub fn new(bios: Bios, gpu_backend: Box<dyn GpuBackend>) -> Self {
        Self {
            bios,
            ram: Ram::new(),
            scratchpad: Scratchpad::new(),
            interrupt_controller: InterruptController::new(),
            dma: DmaController::new(),
            gpu: Gpu::new(gpu_backend),
            cd_rom: CdRom::new(),
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
    0xFFFF_FFFF,
    0xFFFF_FFFF,
    0xFFFF_FFFF,
    0xFFFF_FFFF,
    // KSEG0: 512 MB
    0x7FFF_FFFF,
    // KSEG1: 512 MB
    0x1FFF_FFFF,
    // KSEG2: 1024 MB
    0xFFFF_FFFF,
    0xFFFF_FFFF,
];

/// Mask an address to get the correct region for memory access. The PS1 has 8 regions,
/// each with a different mask. Since there is no virtual memory, the function simply removes the
/// region bits from the address to get the correct physical address for memory access.
pub fn mask_region(addr: u32) -> u32 {
    let region = (addr >> 29) as usize; // Get the top 3 bits to determine the region
    addr & REGION_MASK[region] // Mask the address to get the correct physical address
}

macro_rules! bus_read {
    ($self:expr, $addr:expr, $method:ident, $width:expr) => {{
        let addr = mask_region($addr);

        if let Some(offset) = RAM.contains(addr) {
            return $self.ram.$method(offset);
        }
        if let Some(offset) = BIOS.contains(addr) {
            return $self.bios.$method(offset);
        }
        if let Some(offset) = SCRATCHPAD.contains(addr) {
            return $self.scratchpad.$method(offset);
        }
        if let Some(offset) = CDROM_REGISTERS.contains(addr) {
            return $self.cd_rom.read_register(offset) as _;
        }
        if (addr >= 0x1F80_1040 && addr < 0x1F80_105E) {
            return 0xFFFF_FFFFu32 as _;
        }

        let word = $self.read_hardware(addr);
        let shift = match $width {
            AccessWidth::Byte => (addr & 3) * 8,
            AccessWidth::Half => (addr & 1) * 8,
            AccessWidth::Word => 0,
        };
        return (word >> shift) as _;
    }};
}

macro_rules! bus_write {
    ($self:expr, $addr:expr, $value:expr, $method:ident, $width:expr) => {{
        let addr = mask_region($addr);

        if let Some(offset) = RAM.contains(addr) {
            return $self.ram.$method(offset, $value);
        }
        if let Some(offset) = SCRATCHPAD.contains(addr) {
            return $self.scratchpad.$method(offset, $value);
        }
        if let Some(offset) = CDROM_REGISTERS.contains(addr) {
            return $self.cd_rom.write_register(offset, $value as u8);
        }
        if let Some(_) = EXP2.contains(addr) {
            return;
        }

        if (addr >= 0x1F80_1040 && addr < 0x1F80_105E) {
            return;
        }
        $self.write_hardware(addr, $value as u32, $width);
    }};
}

impl SystemBus {
    fn read_hardware(&mut self, addr: u32) -> u32 {
        if let Some(_offset) = EXP1.contains(addr) {
            return 0xFF;
        }
        if let Some(offset) = MEMORY_CONTROL_1.contains(addr) {
            return self.read_memory_control_1(offset);
        }
        if addr == 0x1F80_1060 {
            return 0x0000_0B88;
        } // RAM_SIZE
        if let Some(offset) = INTERRUPT_CONTROL.contains(addr) {
            return self.read_interrupt_control(offset);
        }
        if let Some(offset) = DMA_REGISTERS.contains(addr) {
            return self.dma.read_register(offset);
        }
        if let Some(_offset) = TIMERS.contains(addr) {
            return 0xFFFF_FFFF;
        } // TODO timers
        if let Some(offset) = GPU_REGISTERS.contains(addr) {
            return self.read_gpu_register(offset);
        }
        if let Some(_offset) = SPU_REGISTERS.contains(addr) {
            return 0;
        } // TODO spu

        eprintln!("Unhandled read at address {addr:#x}");
        0
    }

    fn write_hardware(&mut self, addr: u32, value: u32, width: AccessWidth) {
        let value = match width {
            AccessWidth::Word => value,
            AccessWidth::Half => {
                let shift = (addr & 1) * 8;
                let old = self.read_hardware(addr & !1);
                (old & !(0xFFFF << shift)) | (value << shift)
            }
            AccessWidth::Byte => {
                let shift = (addr & 3) * 8;
                let old = self.read_hardware(addr & !3);
                (old & !(0xFF << shift)) | (value << shift)
            }
        };
        let addr = match width {
            AccessWidth::Word => addr,
            AccessWidth::Half => addr & !1,
            AccessWidth::Byte => addr & !3,
        };

        if let Some(_offset) = MEMORY_CONTROL_1.contains(addr) {
            return;
        } // Absorb writes to memory control 1, just gonna hardware. HACK
        if addr == 0x1F80_1060 {
            return;
        } // Absorb writes to RAM_SIZE
        if let Some(offset) = INTERRUPT_CONTROL.contains(addr) {
            return self.write_interrupt_control(offset, value as u16);
        }
        if let Some(offset) = DMA_REGISTERS.contains(addr) {
            return self.dma.write_register(offset, value);
        }
        if let Some(_offset) = TIMERS.contains(addr) {
            return;
        } // TODO timers
        if let Some(offset) = GPU_REGISTERS.contains(addr) {
            return self.write_gpu_register(offset, value);
        }
        if let Some(_offset) = SPU_REGISTERS.contains(addr) {
            return;
        } // TODO spu

        if addr == 0xFFFE_0130 {
            return;
        } // Absorb writes to the cache control, doesn't matter. HACK
        eprintln!("Unhandled write at address {addr:#x} with value {value:#x}");
    }

    fn read_memory_control_1(&self, offset: u32) -> u32 {
        /*
        Memory Control 1
        1F801000h 4    Expansion 1 Base Address (usually 1F000000h)
        1F801004h 4    Expansion 2 Base Address (usually 1F802000h)
        1F801008h 4    Expansion 1 Delay/Size (usually 0013243Fh; 512Kbytes 8bit-bus)
        1F80100Ch 4    Expansion 3 Delay/Size (usually 00003022h; 1 byte)
        1F801010h 4    BIOS ROM    Delay/Size (usually 0013243Fh; 512Kbytes 8bit-bus)
        1F801014h 4    SPU_DELAY   Delay/Size (usually 200931E1h)
        1F801018h 4    CDROM_DELAY Delay/Size (usually 00020843h or 00020943h)
        1F80101Ch 4    Expansion 2 Delay/Size (usually 00070777h; 128-bytes 8bit-bus)
        1F801020h 4    COM_DELAY / COMMON_DELAY (00031125h or 0000132Ch or 00001325h)
         */

        match offset {
            0 => 0x1F00_0000,    // Expansion 1 Base Address
            4 => 0x1F80_2000,    // Expansion 2 Base Address
            8 => 0x0013_243F,    // Expansion 1 Delay/Size
            0xC => 0x0000_3022,  // Expansion 3 Delay/Size
            0x10 => 0x0013_243F, // BIOS ROM Delay/Size
            0x14 => 0x2009_31E1, // SPU_DELAY
            0x18 => 0x0002_0843, // CDROM_DELAY
            0x1C => 0x0007_0777, // Expansion 2 Delay/Size
            0x20 => 0x0003_1125, // COM_DELAY / COMMON_DELAY
            _ => {
                eprintln!("Unhandled read from memory control 1 at offset {offset:#x}");
                0
            }
        }
    }

    fn read_interrupt_control(&self, offset: u32) -> u32 {
        match offset {
            0 => self.interrupt_controller.read_stat(),
            4 => self.interrupt_controller.read_mask(),
            _ => {
                eprintln!("Unhandled read from interrupt controller at offset {offset:#x}");
                0
            }
        }
    }

    fn write_interrupt_control(&mut self, offset: u32, value: u16) {
        match offset {
            0 => self.interrupt_controller.write_stat(value),
            4 => self.interrupt_controller.write_mask(value),
            _ => eprintln!(
                "Unhandled write to interrupt controller at offset {offset:#x} with value {value:#x}"
            ),
        }
    }

    fn read_gpu_register(&mut self, offset: u32) -> u32 {
        self.gpu.read_register(offset)
    }

    fn write_gpu_register(&mut self, offset: u32, value: u32) {
        self.gpu.write_register(offset, value);
    }
}

impl SystemBus {
    pub fn read32(&mut self, addr: u32) -> u32 {
        bus_read!(self, addr, read32, AccessWidth::Word)
    }
    pub fn read16(&mut self, addr: u32) -> u16 {
        bus_read!(self, addr, read16, AccessWidth::Half)
    }
    pub fn read8(&mut self, addr: u32) -> u8 {
        bus_read!(self, addr, read8, AccessWidth::Byte)
    }
    pub fn write32(&mut self, addr: u32, value: u32) {
        bus_write!(self, addr, value, write32, AccessWidth::Word)
    }
    pub fn write16(&mut self, addr: u32, value: u16) {
        bus_write!(self, addr, value, write16, AccessWidth::Half)
    }
    pub fn write8(&mut self, addr: u32, value: u8) {
        bus_write!(self, addr, value, write8, AccessWidth::Byte)
    }
}

// DMA transfer types
impl SystemBus {
    fn dma_otc(&mut self, base_addr: u32, word_count: u32) {
        let mut addr = base_addr & 0x001F_FFFF;
        for i in 0..word_count {
            let val = if i == word_count - 1 {
                0x00FF_FFFF // end marker at the lowest address entry
            } else {
                // point to previous entry (addr - 4)
                addr.wrapping_sub(4) & 0x00FF_FFFF
            };
            self.ram.write32(addr, val);
            addr = addr.wrapping_sub(4) & 0x001F_FFFF;
        }
    }

    fn dma_cdrom_to_ram(&mut self, dest_addr: u32, word_count: u32) {
        let mut addr = dest_addr & 0x001F_FFFC;
        for _ in 0..word_count {
            let b0 = self.cd_rom.read_data_fifo() as u32;
            let b1 = self.cd_rom.read_data_fifo() as u32;
            let b2 = self.cd_rom.read_data_fifo() as u32;
            let b3 = self.cd_rom.read_data_fifo() as u32;
            let word = b0 | (b1 << 8) | (b2 << 16) | (b3 << 24);

            self.ram.write32(addr, word);
            addr = (addr + 4) & 0x001F_FFFC;
        }
    }

    fn dma_gpu_linked_list(&mut self, base_addr: u32) {
        let mut addr = base_addr & 0x001F_FFFC;
        loop {
            let header = self.ram.read32(addr);
            let word_count = (header >> 24) as u32;

            // Traverse the linked list
            for _ in 0..word_count {
                addr = (addr + 4) & 0x001F_FFFC;
                let cmd = self.ram.read32(addr);
                self.gpu.write_register(0, cmd); // Write to GP0
            }

            if header & 0x0080_0000 != 0 {
                break;
            }

            addr = header & 0x001F_FFFC;
        }
    }

    fn dma_gpu_vram_write(&mut self, base_addr: u32, word_count: u32) {
        let mut addr = base_addr & 0x001F_FFFC;
        for _ in 0..word_count {
            let data = self.ram.read32(addr);
            self.gpu.write_register(0, data); // Write to GP0
            addr = addr.wrapping_add(4);
        }
    }

    fn dma_gpu_vram_read(&mut self, base_addr: u32, word_count: u32) {
        let mut addr = base_addr & 0x001F_FFFC;
        for _ in 0..word_count {
            // TODO should be an Option
            let data = self.gpu.read_register(0); // Read from GPUREAD
            // let data = 0x12341234u32;
            self.ram.write32(addr, data);
            addr = addr.wrapping_add(4); // Wrap around the 2MB RAM size
        }
    }
}

// DMA dispatch
impl SystemBus {
    fn execute_dma_transfer(&mut self, transfer: DmaTransfer) {
        match transfer {
            DmaTransfer::OtcFill {
                base_addr,
                word_count,
            } => self.dma_otc(base_addr, word_count),
            DmaTransfer::CdromToRam {
                dest_addr,
                word_count,
            } => self.dma_cdrom_to_ram(dest_addr, word_count),
            DmaTransfer::GpuLinkedList { list_addr } => self.dma_gpu_linked_list(list_addr),
            DmaTransfer::GpuVramWrite {
                src_addr,
                word_count,
            } => self.dma_gpu_vram_write(src_addr, word_count),
            DmaTransfer::GpuVramRead {
                dest_addr,
                word_count,
            } => self.dma_gpu_vram_read(dest_addr, word_count),
            _ => eprintln!("Unhandled DMA transfer type: {:?}", transfer),
        }
    }

    fn tick_dma(&mut self) {
        while let Some((channel, transfer)) = self.dma.get_pending_transfer() {
            self.execute_dma_transfer(transfer);
            self.dma
                .complete_transfer(channel, &mut self.interrupt_controller);
        }
    }
}

// Hardware access exposed to the top level
impl SystemBus {
    /// Tick the system bus, which will tick the DMA controller and any other peripherals that need to be updated.
    pub fn tick(&mut self, cycles: u32) {
        self.tick_dma();
        self.cd_rom.tick(cycles, &mut self.interrupt_controller);
    }

    /// Insert a disc into the CD-ROM drive from the provided .cue file path.
    pub fn insert_cdrom_disc(&mut self, path: &str) {
        self.cd_rom.insert_disc(path);
    }
}

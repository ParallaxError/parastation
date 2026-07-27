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
use crate::interrupt_controller::{Interrupt, InterruptController};
use crate::mdec::Mdec;
use crate::memory_map::*;
use crate::ram::Ram;
use crate::scheduler::{Scheduler, SchedulerEvent};
use crate::scratchpad::Scratchpad;
use crate::sio0::{InputProvider, SioController};
use crate::spu::{Spu, SpuBackend};
use crate::timers::Timers;
use crate::{SPU_CYCLES, VBLANK_CYCLES};

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
    scheduler: Scheduler,

    // Owned memories
    bios: Bios,
    ram: Ram,
    scratchpad: Scratchpad,
    pub sio: SioController,
    pub interrupt_controller: InterruptController,
    dma: DmaController,
    pub gpu: Gpu,
    pub spu: Spu,
    cd_rom: CdRom,
    timers: Timers,
    mdec: Mdec,
}

impl SystemBus {
    pub fn new(
        bios: Bios,
        gpu_backend: Box<dyn GpuBackend>,
        spu_backend: Box<dyn SpuBackend>,
        joy1: Box<dyn InputProvider>,
        joy2: Box<dyn InputProvider>,
    ) -> Self {
        // Create scheduler and schedule an initial VBlank interrupt to occur
        let mut scheduler = Scheduler::new();
        scheduler.schedule(SchedulerEvent::VBlank, VBLANK_CYCLES);
        // Also schedule an initial SPU tick to occur
        scheduler.schedule(SchedulerEvent::Spu, SPU_CYCLES);

        Self {
            scheduler,
            bios,
            ram: Ram::new(),
            scratchpad: Scratchpad::new(),
            sio: SioController::new(joy1, joy2),
            interrupt_controller: InterruptController::new(),
            dma: DmaController::new(),
            gpu: Gpu::new(gpu_backend),
            spu: Spu::new(spu_backend),
            cd_rom: CdRom::new(),
            timers: Timers::new(),
            mdec: Mdec::new(),
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
            return $self.ram.$method(offset & 0x1F_FFFF);
        }
        if let Some(offset) = BIOS.contains(addr) {
            return $self.bios.$method(offset);
        }
        if let Some(offset) = SCRATCHPAD.contains(addr) {
            return $self.scratchpad.$method(offset);
        }
        if let Some(offset) = SIO0_REGISTERS.contains(addr) {
            return $self.sio.read_register(offset) as _;
        }
        if let Some(offset) = CDROM_REGISTERS.contains(addr) {
            return $self.cd_rom.read_register(offset) as _;
        }
        if let Some(offset) = SPU_REGISTERS.contains(addr) {
            return $self
                .spu
                .read_register(offset, &mut $self.interrupt_controller) as _;
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
            return $self.ram.$method(offset & 0x1F_FFFF, $value);
        }
        if let Some(offset) = SCRATCHPAD.contains(addr) {
            return $self.scratchpad.$method(offset, $value);
        }
        if let Some(offset) = SIO0_REGISTERS.contains(addr) {
            return $self
                .sio
                .write_register(offset, $value as u32, &mut $self.scheduler);
        }
        if let Some(offset) = CDROM_REGISTERS.contains(addr) {
            return $self
                .cd_rom
                .write_register(offset, $value as u8, &mut $self.scheduler);
        }
        if let Some(offset) = SPU_REGISTERS.contains(addr) {
            return $self.spu.write_register(
                offset,
                $value as u16,
                &mut $self.interrupt_controller,
            );
        }
        if let Some(_) = EXP2.contains(addr) {
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
        if let Some(offset) = TIMERS.contains(addr) {
            let timer = (offset / 0x10) as usize;
            let timer_offset = offset % 0x10;
            return self.timers.read_register(timer, timer_offset);
        }
        if let Some(offset) = GPU_REGISTERS.contains(addr) {
            return self.read_gpu_register(offset);
        }
        if let Some(offset) = MDEC_REGISTERS.contains(addr) {
            return self.mdec.read_register(offset);
        }

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
            self.dma
                .write_register(offset, value, &mut self.interrupt_controller);
            if DmaController::is_chcr(offset) {
                self.tick_dma();
            }
            return;
        }
        if let Some(offset) = TIMERS.contains(addr) {
            let timer = (offset / 0x10) as usize;
            let timer_offset = offset % 0x10;
            return self
                .timers
                .write_register(timer, timer_offset, value as u16);
        }
        if let Some(offset) = GPU_REGISTERS.contains(addr) {
            return self.write_gpu_register(offset, value);
        }
        if let Some(offset) = MDEC_REGISTERS.contains(addr) {
            return self.mdec.write_register(offset, value);
        }

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
            let data = self.gpu.read_register(0); // Read from GPUREAD
            self.ram.write32(addr, data);
            addr = addr.wrapping_add(4); // Wrap around the 2MB RAM size
        }
    }

    fn dma_spu_write(&mut self, base_addr: u32, word_count: u32) {
        let mut addr = base_addr & 0x001F_FFFC;
        for _ in 0..word_count {
            let word = self.ram.read32(addr);

            let lo = (word & 0xFFFF) as u16;
            let hi = (word >> 16) as u16;

            self.spu.write_data_port(lo, &mut self.interrupt_controller);
            self.spu.write_data_port(hi, &mut self.interrupt_controller);
            addr = addr.wrapping_add(4); // Wrap around the 2MB RAM size
        }
    }

    fn dma_spu_read(&mut self, base_addr: u32, word_count: u32) {
        let mut addr = base_addr & 0x001F_FFFC;
        for _ in 0..word_count {
            let lo = self.spu.read_data_port(&mut self.interrupt_controller);
            let hi = self.spu.read_data_port(&mut self.interrupt_controller);

            let word = (hi as u32) << 16 | (lo as u32);
            self.ram.write32(addr, word);
            addr = addr.wrapping_add(4); // Wrap around the 2MB RAM size
        }
    }

    fn dma_mdec_in(&mut self, base_addr: u32, word_count: u32) {
        let mut addr = base_addr & 0x001F_FFFC;
        let mut remaining = word_count;

        while remaining > 0 && self.mdec.wants_more_input() {
            let block_words = remaining.min(0x20);
            for _ in 0..block_words {
                let word = self.ram.read32(addr);
                self.mdec.write_command_param(word);
                addr = addr.wrapping_add(4);
            }
            remaining -= block_words;
        }
    }

    fn dma_mdec_out(&mut self, base_addr: u32, word_count: u32) {
        let mut addr = base_addr & 0x001F_FFFC;
        let mut remaining = word_count;

        while remaining > 0 && self.mdec.has_output_ready() {
            let (word, byte_offset) = self.mdec.dma_read_data_out();
            let write_addr = (addr.wrapping_add(byte_offset)) & 0x001F_FFFC;
            self.ram.write32(write_addr, word);
            addr = addr.wrapping_add(4);
            remaining -= 1;
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
            DmaTransfer::SpuWrite {
                src_addr,
                word_count,
            } => self.dma_spu_write(src_addr, word_count),
            DmaTransfer::SpuRead {
                dest_addr,
                word_count,
            } => self.dma_spu_read(dest_addr, word_count),
            DmaTransfer::MdecIn {
                src_addr,
                word_count,
            } => self.dma_mdec_in(src_addr, word_count),
            DmaTransfer::MdecOut {
                dest_addr,
                word_count,
            } => self.dma_mdec_out(dest_addr, word_count),
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
    /// Tick the system bus, advancing the scheduler and processing any pending peripheral events
    pub fn tick(&mut self, cycles: u32) {
        // TODO why u32 and u64 mix?
        self.timers
            .tick(cycles as u64, &mut self.interrupt_controller);
        let to_service = self.scheduler.advance(cycles);

        // Process any events that require servicing, hopefully slippage was minimal
        for event in to_service {
            match event {
                SchedulerEvent::VBlank => {
                    self.interrupt_controller.raise_interrupt(Interrupt::VBlank);
                    self.scheduler
                        .schedule(SchedulerEvent::VBlank, VBLANK_CYCLES);
                }
                SchedulerEvent::CdRomResponse { bytes, int_code } => {
                    self.cd_rom.handle_response_event(
                        bytes,
                        int_code,
                        &mut self.scheduler,
                        &mut self.interrupt_controller,
                    );
                }
                SchedulerEvent::CdRomSectorRead => {
                    self.cd_rom.handle_sector_read_event(
                        &mut self.scheduler,
                        &mut self.interrupt_controller,
                    );
                }
                SchedulerEvent::SioResponse { byte, dsr } => {
                    self.sio
                        .handle_event(byte, dsr, &mut self.interrupt_controller);
                }
                SchedulerEvent::Spu => {
                    let cd_sample = self.cd_rom.pull_cd_audio_sample();
                    self.spu
                        .handle_event(cd_sample, &mut self.interrupt_controller);
                    self.scheduler.schedule(SchedulerEvent::Spu, SPU_CYCLES);
                }
            }
        }
    }

    /// Insert a disc into the CD-ROM drive from the provided .cue file path.
    pub fn insert_cdrom_disc(&mut self, path: &str) {
        self.cd_rom.insert_disc(path);
    }
}

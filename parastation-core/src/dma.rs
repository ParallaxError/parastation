/*
 * @file /parastation-core/src/dma.rs
 * @brief
 * Direct Memory Access (DMA) controller for the PS1. Handles DMA transfers between peripherals, 
 * and has 7 DMA channels for different peripherals (GPU, SPU, CD-ROM, etc.).
 * 
 * The DMA controller produces transfer requests, but the system bus actually performs the transfers, 
 * so the controller just needs to keep track of the settings and create the requests.
 * 
 * -----
 */

/// Possible DMA transfer types that the system bus should execute, decoded by the DMA controller
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DmaTransfer {
    /// SyncMode 0, DMA6: Fill RAM backwards with the OTC (Ordering Table Clear) value, used for clearing GPU command linked lists
    OtcFill { base_addr: u32, word_count: u32 },
    /// SyncMode 0, DMA3: CDROM data to RAM, a burst read from the CDROM FIFO to the RAM
    CdromToRam { dest_addr: u32, word_count: u32 },
    /// SyncMode 1, DMA2: CPU/RAM to GPU VRAM (image data)
    GpuVramWrite { src_addr: u32, word_count: u32 },
    /// SyncMode 1, DMA2: GPU VRAM to CPU/RAM (image data)
    GpuVramRead { dest_addr: u32, word_count: u32 },
    /// SyncMode 2, DMA2: GPU command list (ordering table). Walks the linked list and sends each node's words to GP0
    GpuLinkedList { list_addr: u32 },
    /// SyncMode 1, DMA4: RAM to SPU sound RAM
    SpuWrite { src_addr: u32, word_count: u32 },
    /// SyncMode 1, DMA4: SPU sound RAM to RAM
    SpuRead { dest_addr: u32, word_count: u32 },
    /// SyncMode 1, DMA0: RAM to MDEC0 compressed data
    MdecIn { src_addr: u32, word_count: u32 },
    /// SyncMode 1, DMA1: MDEC0 decompressed data to RAM
    MdecOut { dest_addr: u32, word_count: u32 },
}

// DMA channel member structures
// https://problemkaputt.de/psx-spx.htm#dmachannels
pub struct DmaBlockControl (
    pub u32 // 32 bit register, meaning of bits depend on SyncMode
);

impl DmaBlockControl {
    // SyncMode=0: number of words
    pub fn word_count(&self) -> u32 {
        let bc = self.0 & 0xFFFF;
        if bc == 0 { 0x10000 } else { bc as u32 }
    }

    // SyncMode=1: blocksize and block count
    pub fn block_size(&self) -> u32 {
        let bs = self.0 & 0xFFFF;
        if bs == 0 { 0x10000 } else { bs as u32 }
    }

    pub fn block_count(&self) -> u32 {
        let ba = (self.0 >> 16) & 0xFFFF;
        if ba == 0 { 0x10000 } else { ba as u32 }
    }

    pub fn total_words(&self) -> u32 {
        self.block_size() * self.block_count()
    }
}

// DMA channel control structures
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransferDirection {
    ToRam = 0,
    FromRam = 1,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemoryStep {
    Forward = 0,
    Backward = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncMode {
    ImmediateStartTransfer = 0,
    SyncBlocksToRequests = 1,
    LinkedListMode = 2,
    Reserved = 3,
}

pub struct DmaChannelControl {
    pub direction: TransferDirection,
    pub memory_step: MemoryStep,
    pub enable_chopping: bool, // 1 = run CPU during DMA gaps
    pub sync_mode: SyncMode,
    pub chopping_dma_window_size: u8, // 0-15, 2^n words
    pub chopping_cpu_window_size: u8, // 0-15, 2^n cycles
    pub busy: bool, // Whether the channel is currently active
    pub start_trigger: bool // 0 = normal, 1 = manual start (for sync mode 0)
}

impl DmaChannelControl {
    pub fn new() -> Self {
        Self {
            direction: TransferDirection::ToRam,
            memory_step: MemoryStep::Forward,
            enable_chopping: false,
            sync_mode: SyncMode::ImmediateStartTransfer,
            chopping_dma_window_size: 0,
            chopping_cpu_window_size: 0,
            busy: false,
            start_trigger: false,
        }
    }

    /*
    0       Transfer Direction    (0=To Main RAM, 1=From Main RAM)
    1       Memory Address Step   (0=Forward;+4, 1=Backward;-4)
    2-7     Not used              (always zero)
    8       Chopping Enable       (0=Normal, 1=Chopping; run CPU during DMA gaps)
    9-10    SyncMode, Transfer Synchronisation/Mode (0-3):
                0  Start immediately and transfer all at once (used for CDROM, OTC)
                1  Sync blocks to DMA requests   (used for MDEC, SPU, and GPU-data)
                2  Linked-List mode              (used for GPU-command-lists)
                3  Reserved                      (not used)
    11-15   Not used              (always zero)
    16-18   Chopping DMA Window Size (1 SHL N words)
    19      Not used              (always zero)
    20-22   Chopping CPU Window Size (1 SHL N clks)
    23      Not used              (always zero)
    24      Start/Busy            (0=Stopped/Completed, 1=Start/Enable/Busy)
    25-27   Not used              (always zero)
    28      Start/Trigger         (0=Normal, 1=Manual Start; use for SyncMode=0)
    29      Unknown (R/W) Pause?  (0=No, 1=Pause?)     (For SyncMode=0 only?)
    30      Unknown (R/W)
    31      Not used              (always zero)
     */

    pub fn from_word(word: u32) -> Self {
        Self {
            direction: if (word & 0x1) == 0 { TransferDirection::ToRam } else { TransferDirection::FromRam },
            memory_step: if (word & 0x2) == 0 { MemoryStep::Forward } else { MemoryStep::Backward },
            enable_chopping: (word & 0x100) != 0,
            sync_mode: match (word >> 9) & 0x3 {
                0 => SyncMode::ImmediateStartTransfer,
                1 => SyncMode::SyncBlocksToRequests,
                2 => SyncMode::LinkedListMode,
                _ => SyncMode::Reserved,
            },
            chopping_dma_window_size: ((word >> 16) & 0x7) as u8,
            chopping_cpu_window_size: ((word >> 20) & 0x7) as u8,
            busy: (word & 0x1000000) != 0,
            start_trigger: (word & 0x10000000) != 0,
        }
    }

    pub fn to_word(&self) -> u32 {
        let mut word = 0;
        word |= match self.direction {
            TransferDirection::ToRam => 0,
            TransferDirection::FromRam => 1,
        };
        word |= match self.memory_step {
            MemoryStep::Forward => 0,
            MemoryStep::Backward => 2,
        };
        if self.enable_chopping { word |= 0x100; }
        word |= match self.sync_mode {
            SyncMode::ImmediateStartTransfer => 0,
            SyncMode::SyncBlocksToRequests => 1 << 9,
            SyncMode::LinkedListMode => 2 << 9,
            SyncMode::Reserved => 3 << 9,
        };
        word |= (self.chopping_dma_window_size as u32) << 16;
        word |= (self.chopping_cpu_window_size as u32) << 20;
        if self.busy { word |= 0x1000000; }
        if self.start_trigger { word |= 0x10000000; }
        word
    }
}

/// DMA channel which can be used for DMA transfers between the RAM and a peripheral.
/// Encodes the settings of the channel and whether a channel is active
pub struct DmaChannel {
    pub madr: u32, // Base address
    pub bcr: DmaBlockControl,
    pub chcr: DmaChannelControl,
}

impl DmaChannel {
    pub fn new() -> Self {
        Self {
            madr: 0,
            bcr: DmaBlockControl(0),
            chcr: DmaChannelControl::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.chcr.busy && match self.chcr.sync_mode {
            SyncMode::ImmediateStartTransfer => self.chcr.start_trigger, // For sync mode 0, the channel is active if start_trigger is set
            _ => true, // For other sync modes, the channel is active as long as busy
        }
    }
}

/// DMA controller for the PS1, which handles DMA transfers between peripherals. The PS1 has 7 DMA
/// channels for different peripherals (GPU, SPU, CD-ROM, etc.).
/// 
/// The DMA controller produces transfer requests, but the system bus actually performs the transfers, 
/// so the controller just needs to keep track of the settings and create the requests.
pub struct DmaController {
    pub channels: [DmaChannel; 7],
    pub priorities: [u8; 7], // Priority of each channel, 0-7 (0 = highest priority)
    pub enables: [bool; 7], // Whether each channel is enabled
    pub dicr: u32, // DMA Interrupt Register
}

impl DmaController {
    pub fn new() -> Self {
        Self {
            channels: [
                DmaChannel::new(), // MDECin
                DmaChannel::new(), // MDECout
                DmaChannel::new(), // GPU
                DmaChannel::new(), // CDROM
                DmaChannel::new(), // SPU
                DmaChannel::new(), // PIO (parallel I/O, used for controllers)
                DmaChannel::new(), // OTC (Ordering Table Clear, used for clearing GPU command linked lists)
            ],
            priorities: [0; 7],
            enables: [false; 7],
            dicr: 0,
        }
    }

    // Register reads/writes
    /*
    1F8010F0h - DPCR - DMA Control Register (R/W)
    0-2   DMA0, MDECin  Priority      (0..7; 0=Highest, 7=Lowest)
    3     DMA0, MDECin  Master Enable (0=Disable, 1=Enable)
    4-6   DMA1, MDECout Priority      (0..7; 0=Highest, 7=Lowest)
    7     DMA1, MDECout Master Enable (0=Disable, 1=Enable)
    8-10  DMA2, GPU     Priority      (0..7; 0=Highest, 7=Lowest)
    11    DMA2, GPU     Master Enable (0=Disable, 1=Enable)
    12-14 DMA3, CDROM   Priority      (0..7; 0=Highest, 7=Lowest)
    15    DMA3, CDROM   Master Enable (0=Disable, 1=Enable)
    16-18 DMA4, SPU     Priority      (0..7; 0=Highest, 7=Lowest)
    19    DMA4, SPU     Master Enable (0=Disable, 1=Enable)
    20-22 DMA5, PIO     Priority      (0..7; 0=Highest, 7=Lowest)
    23    DMA5, PIO     Master Enable (0=Disable, 1=Enable)
    24-26 DMA6, OTC     Priority      (0..7; 0=Highest, 7=Lowest)
    27    DMA6, OTC     Master Enable (0=Disable, 1=Enable)
    28-30 Unknown, Priority Offset or so? (R/W)
    31    Unknown, no effect? (R/W)
    */

    fn read_dpcr(&self) -> u32 {

        let mut value = 0;
        for i in 0..7 {
            value |= (self.priorities[i] as u32 & 0x7) << (i * 4);
            if self.enables[i] {
                value |= 1 << (i * 4 + 3);
            }
        }
        value
    }

    fn write_dpcr(&mut self, value: u32) {
        for i in 0..7 {
            self.priorities[i] = ((value >> (i * 4)) & 0x7) as u8;
            self.enables[i] = (value & (1 << (i * 4 + 3))) != 0;
        }
    }

    fn read_dicr(&self) -> u32 {
        self.dicr
    }

    fn write_dicr(&mut self, value: u32) {
        self.dicr = value;
    }

    fn read_dma_base_addr(&self, channel: usize) -> u32 {
        self.channels[channel].madr
    }

    fn read_dma_block_control(&self, channel: usize) -> u32 {
        self.channels[channel].bcr.0
    }

    fn read_dma_channel_control(&self, channel: usize) -> u32 {
        self.channels[channel].chcr.to_word()
    }

    fn write_dma_base_addr(&mut self, channel: usize, value: u32) {
        self.channels[channel].madr = value;
    }

    fn write_dma_block_control(&mut self, channel: usize, value: u32) {
        self.channels[channel].bcr = DmaBlockControl(value);
    }

    fn write_dma_channel_control(&mut self, channel: usize, value: u32) {
        self.channels[channel].chcr = DmaChannelControl::from_word(value);
    }

    // Read/write dispatch
    /*
    DMA Register Summary
    1F80108xh DMA0 channel 0  MDECin  (RAM to MDEC)
    1F80109xh DMA1 channel 1  MDECout (MDEC to RAM)
    1F8010Axh DMA2 channel 2  GPU (lists + image data)
    1F8010Bxh DMA3 channel 3  CDROM   (CDROM to RAM)
    1F8010Cxh DMA4 channel 4  SPU
    1F8010Dxh DMA5 channel 5  PIO (Expansion Port)
    1F8010Exh DMA6 channel 6  OTC (reverse clear OT) (GPU related)
    1F801080h+N*10h - D#_MADR - DMA base address (Channel 0..6) (R/W)
    1F801084h+N*10h - D#_BCR - DMA Block Control (Channel 0..6) (R/W)
    1F801088h+N*10h - D#_CHCR - DMA Channel Control (Channel 0..6) (R/W)
    1F8010F0h DPCR - DMA Control register
    1F8010F4h DICR - DMA Interrupt register
     */


    /// Register offsets are relative to the DMA register block base (0x1F80_1080).
    /// In other words: offset 0x00 maps to 0x1F80_1080 (DMA0_MADR).
    pub fn read_register(&self, offset: u32) -> u32 {
        match offset {
            // 0x00..=0x60: D#_MADR (channel 0..6)
            0x00..=0x60 if offset % 0x10 == 0 => {
                let channel = offset / 0x10;
                self.read_dma_base_addr(channel as usize)
            }
            // 0x04..=0x64: D#_BCR (channel 0..6)
            0x04..=0x64 if offset % 0x10 == 4 => {
                let channel = (offset - 0x04) / 0x10;
                self.read_dma_block_control(channel as usize)
            }
            // 0x08..=0x68: D#_CHCR (channel 0..6)
            0x08..=0x68 if offset % 0x10 == 8 => {
                let channel = (offset - 0x08) / 0x10;
                self.read_dma_channel_control(channel as usize)
            }
            // 0x70/0x74: DPCR/DICR
            0x70 => self.read_dpcr(),
            0x74 => self.read_dicr(),
            _ => {
                eprintln!("Invalid DMA register read at offset {offset:#x}");
                0
            }
        }
    }

    pub fn write_register(&mut self, offset: u32, value: u32) {
        match offset {
            0x00..=0x60 if offset % 0x10 == 0 => {
                let channel = offset / 0x10;
                self.write_dma_base_addr(channel as usize, value);
            }
            0x04..=0x64 if offset % 0x10 == 4 => {
                let channel = (offset - 0x04) / 0x10;
                self.write_dma_block_control(channel as usize, value);
            }
            0x08..=0x68 if offset % 0x10 == 8 => {
                let channel = (offset - 0x08) / 0x10;
                self.write_dma_channel_control(channel as usize, value);
            }
            0x70 => self.write_dpcr(value),
            0x74 => self.write_dicr(value),
            _ => eprintln!("Invalid DMA register write at offset {offset:#x} with value {value:#x}"),
        }
    }

    pub fn complete_transfer(&mut self, channel: usize) {
        let ch = &mut self.channels[channel];

        // Clear busy and trigger bits
        ch.chcr.busy = false;
        ch.chcr.start_trigger = false;

        // Set interrupt flag if enabled for this channel
        let irq_enabled = (self.dicr >> (16 + channel)) & 0x1 != 0;
        if irq_enabled {
            self.dicr |= 1 << (24 + channel);
        }

        // Recalculate master IRQ flag per spec:
        // IF b15=1 OR (b23=1 AND b(24-30)>0) THEN b31=1 ELSE b31=0
        let force_irq    = (self.dicr >> 15) & 0x1 != 0;
        let master_enable = (self.dicr >> 23) & 0x1 != 0;
        let any_flags    = (self.dicr >> 24) & 0x7F != 0;
        if force_irq || (master_enable && any_flags) {
            self.dicr |= 1 << 31;
        } else {
            self.dicr &= !(1 << 31);
        }
    }

    pub fn irq_pending(&self) -> bool {
        (self.dicr >> 31) & 0x1 != 0
    }
}

// DMA transfer request decoding
impl DmaController {
    fn decode_channel(&self, channel: usize) -> Option<DmaTransfer> {
        let ch = &self.channels[channel];
        let addr = ch.madr & 0x00FFFFFF; // Mask to 24 bits

        match channel {
            // DMA6: Ordering Table Clear, so always return an OtcFill transfer
            6 => Some(
                DmaTransfer::OtcFill { base_addr: addr, word_count: ch.bcr.word_count() }
            ),

            // DMA2: GPU, mode depends on the channel control settings
            2 => match ch.chcr.sync_mode {
                SyncMode::LinkedListMode => Some(
                    DmaTransfer::GpuLinkedList { list_addr: addr }
                ),
                SyncMode::SyncBlocksToRequests => {
                    if ch.chcr.direction == TransferDirection::FromRam {
                        Some(DmaTransfer::GpuVramWrite { src_addr: addr, word_count: ch.bcr.total_words() })
                    } else {
                        Some(DmaTransfer::GpuVramRead { dest_addr: addr, word_count: ch.bcr.total_words() })
                    }
                },
                SyncMode::ImmediateStartTransfer => {
                    eprintln!("DMA2 with SyncMode 0 (immediate), treating as VRAM write...");
                    Some(
                        DmaTransfer::GpuVramWrite { src_addr: addr, word_count: ch.bcr.total_words() }
                    )
                },
                _ => { eprintln!("DMA2 unsupported sync mode"); None },
            },

            // DMA3: CDROM, always read from CDROM FIFO to RAM
            3 => Some(
                DmaTransfer::CdromToRam { dest_addr: addr, word_count: ch.bcr.word_count() }
            ),

            // DMA4: SPU, mode depends on the channel control settings
            4 => match ch.chcr.sync_mode {
                SyncMode::ImmediateStartTransfer | SyncMode::SyncBlocksToRequests => {
                    if ch.chcr.direction == TransferDirection::FromRam {
                        Some(DmaTransfer::SpuWrite { src_addr: addr, word_count: ch.bcr.total_words() })
                    } else {
                        Some(DmaTransfer::SpuRead { dest_addr: addr, word_count: ch.bcr.total_words() })
                    }
                }
                _ => { eprintln!("DMA4 unsupported sync mode"); None }
            },

            // DMA0: MDECin, always RAM to MDEC
            0 => Some(
                DmaTransfer::MdecIn { src_addr: addr, word_count: ch.bcr.total_words() }
            ),

            // DMA1: MDECout, always MDEC to RAM
            1 => Some(
                DmaTransfer::MdecOut { dest_addr: addr, word_count: ch.bcr.total_words() }
            ),

            _ => { eprintln!("Unsupported DMA channel {channel}"); None }
        }
    }

    pub fn get_pending_transfer(&self) -> Option<(usize, DmaTransfer)> {
        for i in 0..7 {
            if !self.enables[i] { continue; } // Channel not enabled, skip
            if !self.channels[i].is_active() { continue; } // Channel not active, skip

            let transfer = self.decode_channel(i)?;
            println!("Pending DMA transfer on channel {i}: {transfer:?}");
            return Some((i, transfer));
        }
        None
    }
}
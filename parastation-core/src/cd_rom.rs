/*
 * @file /parastation-core/src/cd_rom.rs
 * @brief
 * Implementation for CD-ROM commands and disk access, based mostly off of
 * https://hitmen.c02.at/files/docs/psx/psx.pdf
 *
 * -----
 */

// Imports
use std::collections::VecDeque;

use crate::interrupt_controller::{Interrupt, InterruptController};
use crate::scheduler::{Scheduler, SchedulerEvent};
use crate::spu::PcmSample;
use crate::xadpcm::{XaSubHeader, XadpcmDecoder};
use crate::{elog, log};

pub const SECTOR_SIZE: usize = 2352; // Size of a CD-ROM sector in bytes

/// Abstracts reading raw bytes from a disc image file, so the frontend can provide its own implementation to make the
/// core platform agnostic.
pub trait DiscSource {
    /// Read buf.len() bytes starting from offset within this specific file. Returns the number of bytes read
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> usize;

    /// Total size of this file in bytes, if known. Used to calculate sector counts.
    fn len(&self) -> u64;
}

/// Inserted disc structure, represented by a file and a list of tracks.
struct Disc {
    files: Vec<Box<dyn DiscSource>>, // File handles for the disc image
    tracks: Vec<Track>,              // List of tracks on the disc
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum TrackType {
    Audio,
    Data,
}

/// Track structure, representing a single track on the disc.
struct Track {
    track_number: u8,
    start_logical_block: u32, // Starting logical block address of the track
    track_type: TrackType,    // Type of the track (Audio or Data)
    file_index: usize,        // Index of the file in the Disc's file vector
    file_offset: u64,         // Byte offset within the file where the track data starts
}

// Response timings
// First response delays
mod cdrom_timing {
    pub const GETSTAT: u64 = 0x000c4e1;
    pub const GETSTAT_STOPPED: u64 = 0x0005cf4;
    pub const INIT_FIRST: u64 = 0x0013cce;
    pub const DEFAULT_FIRST: u64 = 0x000c4e1; // Fallback, need to test all commands

    // Second response delays
    pub const GETID_SECOND: u64 = 0x0004a00;
    pub const PAUSE_SECOND_1X: u64 = 0x021181c;
    pub const PAUSE_SECOND_2X: u64 = 0x010bd93;
    pub const PAUSE_SECOND_ALREADY_PAUSED: u64 = 0x0001df2;
    pub const STOP_SECOND_1X: u64 = 0x0d38aca;
    pub const STOP_SECOND_2X: u64 = 0x18a6076;
    pub const STOP_SECOND_ALREADY_STOPPED: u64 = 0x0001d7b;

    // INT1 rate (per-sector delay during ReadN/ReadS)
    pub const READ_INT1_1X: u64 = 0x006e1cd;
    pub const READ_INT1_2X: u64 = 0x0036cd2;

    pub const SEEK_DELAY: u64 = 0x1E400; // TODO can calculate accurate seek timing?

    // Number of cycles to wait before retrying a command if the previous response was not acknowledged
    pub const RETRY_CYCLES: u64 = 1000;
}

/// CD-ROM controller to handle commands and disk access.
/// Basedn mostly off of https://hitmen.c02.at/files/docs/psx/psx.pdf
pub struct CdRom {
    register_index: u8,    // Index of the currently selected register (0x1F801800)
    command_args: Vec<u8>, // Arguments for the current command
    command_result: VecDeque<u8>, // Result of the last command
    interrupt_flags: u8,   // Interrupt flags for the CD-ROM controller (lower nybble of CDREG3)
    interrupt_enable: u8,  // IRQ enable mask
    mode: u8,              // Mode set by the Setmode command (0x0E)

    seek_target: Option<u32>, // Target logical block address for seek operations
    current_logical_block: u32, // Current logical block address of the disc head
    reading: bool,            // Currently reading sectors (ReadN/ReadS command)
    playing: bool,            // Currently playing audio (Play command)
    disc: Option<Disc>,       // Currently inserted disc, if any

    sector_buffer: [u8; SECTOR_SIZE], // Buffer for storing the current sector data, useful portion in data_buffer
    data_fifo_loaded: bool, // Set by the "want data" bit, indicates that sector reading FIFO has data
    data_buffer: Vec<u8>,   // Buffer for data FIFO reads
    data_buffer_offset: usize, // Read offset within the data buffer for FIFO reads

    // XA filter parameters from Setmode
    xa_filter_enabled: bool, // set by Setmode bit 3
    xa_filter_file: u8,      // set by Setfilter param 1
    xa_filter_channel: u8,   // set by Setfilter param 2

    // Audio
    pending_cdda_samples: VecDeque<(PcmSample, PcmSample)>, // Pending CDDA samples to be sent to the SPU
    xadpcm_decoder: XadpcmDecoder, // XADPCM decoder for handling XA audio decoding

    cd_vol_ll: u8, // Left-CD-Out to Left-SPU-Input (pending, not yet applied)
    cd_vol_lr: u8, // Left-CD-Out to Right-SPU-Input
    cd_vol_rr: u8, // Right-CD-Out to Right-SPU-Input
    cd_vol_rl: u8, // Right-CD-Out to Left-SPU-Input

    applied_vol_ll: u8,
    applied_vol_lr: u8,
    applied_vol_rr: u8,
    applied_vol_rl: u8,
}

// Helpers

/// Convert from the MSF (Minutes:Seconds:Frames) format to LBA (Logical Block Addressing)
/// https://github.com/opsxcq/psx-cue-sbi-collection
fn msf_to_lba(minutes: u8, seconds: u8, frames: u8) -> u32 {
    (minutes as u32 * 60 + seconds as u32) * 75 + frames as u32
}

fn lba_to_msf_bcd(lba: u32) -> (u8, u8, u8) {
    let lba = lba + 150; // PS1 MSF is offset by 2 seconds (lead-in)
    let minutes = (lba / 75 / 60) as u8;
    let seconds = ((lba / 75) % 60) as u8;
    let sectors = (lba % 75) as u8;
    (
        decimal_to_bcd(minutes),
        decimal_to_bcd(seconds),
        decimal_to_bcd(sectors),
    )
}

fn bcd_to_decimal(bcd: u8) -> u8 {
    ((bcd >> 4) * 10) + (bcd & 0x0F)
}

fn decimal_to_bcd(n: u8) -> u8 {
    ((n / 10) << 4) | (n % 10)
}

// Main interface
impl CdRom {
    /// Creates a new CD-ROM controller instance.
    pub fn new() -> Self {
        CdRom {
            register_index: 0,
            command_args: Vec::new(),
            command_result: VecDeque::new(),
            interrupt_flags: 0,
            interrupt_enable: 0,
            mode: 0,

            seek_target: None,
            current_logical_block: 0,
            reading: false,
            playing: false,
            disc: None,

            sector_buffer: [0u8; SECTOR_SIZE],
            data_fifo_loaded: false,
            data_buffer: Vec::new(),
            data_buffer_offset: 0,

            xa_filter_enabled: false,
            xa_filter_file: 0,
            xa_filter_channel: 0,

            pending_cdda_samples: VecDeque::new(),
            xadpcm_decoder: XadpcmDecoder::new(),

            cd_vol_ll: 0x7F,
            cd_vol_lr: 0x7F,
            cd_vol_rr: 0x7F,
            cd_vol_rl: 0x7F,

            applied_vol_ll: 0x7F,
            applied_vol_lr: 0x7F,
            applied_vol_rr: 0x7F,
            applied_vol_rl: 0x7F,
        }
    }

    /// Read a CUE file from the given path and parse it into a Disc structure
    pub fn insert_disc(
        &mut self,
        cue_content: &str,
        mut open_file: impl FnMut(&str) -> Box<dyn DiscSource>,
    ) {
        // https://github.com/opsxcq/psx-cue-sbi-collection
        let mut files: Vec<Box<dyn DiscSource>> = Vec::new();
        let mut tracks: Vec<Track> = Vec::new();

        let mut current_file_index: usize = 0;
        let mut current_track_number: u8 = 0;
        let mut current_track_type: TrackType = TrackType::Data;
        // Track the LBA where the current file begins so that we can calculate the file offset for each track
        let mut current_file_base_lba: u32 = 0;

        // Now we read tokens as space separated, and match FILEs, TRACKs, and INDEXes
        for line in cue_content.lines() {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }

            match tokens[0] {
                "FILE" => {
                    // FILE "filename.bin" BINARY
                    let start = line.find('"').expect("Missing opening quote in FILE line");
                    let end = line.rfind('"').expect("Missing closing quote in FILE line");
                    let filename = &line[start + 1..end];

                    if let Some(prev_file) = files.last() {
                        let sector_count = (prev_file.len() / SECTOR_SIZE as u64) as u32;
                        current_file_base_lba += sector_count;
                    }

                    let file = open_file(filename);
                    files.push(file);
                    current_file_index = files.len() - 1;
                }
                "TRACK" => {
                    // TRACK 01 AUDIO or TRACK 02 MODE1/2352
                    current_track_number = tokens[1].parse::<u8>().expect("Invalid track number");
                    current_track_type = match tokens[2] {
                        "AUDIO" => TrackType::Audio,
                        _ => TrackType::Data,
                    };
                }
                "INDEX" => {
                    // INDEX 01 mm:ss:ff
                    let index_number: u8 = tokens[1].parse().unwrap();
                    if index_number != 1 {
                        continue;
                    }

                    let time_parts: Vec<&str> = tokens[2].split(':').collect();
                    let minutes: u8 = time_parts[0].parse().unwrap();
                    let seconds: u8 = time_parts[1].parse().unwrap();
                    let frames: u8 = time_parts[2].parse().unwrap();

                    let local_lba = msf_to_lba(minutes, seconds, frames);
                    let file_offset = local_lba as u64 * SECTOR_SIZE as u64;
                    let global_start_lba = current_file_base_lba + local_lba;

                    log!(
                        "Parsed track {}: start_lba={}, file_index={}, file_offset={}",
                        current_track_number,
                        global_start_lba,
                        current_file_index,
                        file_offset
                    );

                    tracks.push(Track {
                        track_number: current_track_number,
                        start_logical_block: global_start_lba,
                        track_type: current_track_type,
                        file_index: current_file_index,
                        file_offset,
                    });
                }
                _ => {
                    log!("Unrecognized CUE command: {}", tokens[0]);
                }
            }
        }

        self.disc = Some(Disc {
            files: files,
            tracks,
        });
    }

    /// Handle a CdRomSectorRead event from the scheduler, reading the next sector and scheduling the next read if
    /// necessary
    pub fn handle_sector_read_event(
        &mut self,
        scheduler: &mut Scheduler,
        interrupt_controller: &mut InterruptController,
    ) {
        // Guard against reading when not in a read state or playing state
        if !self.reading && !self.playing {
            return;
        }

        if self.interrupt_flags != 0 {
            // Previous response not acknowledged, so need to retry later
            scheduler.schedule(SchedulerEvent::CdRomSectorRead, cdrom_timing::RETRY_CYCLES);
            return;
        }

        let should_interrupt = self.perform_sector_read(scheduler);

        // Only interrupt for data sectors
        if should_interrupt {
            self.push_response(&[self.get_status_byte()]);
            self.set_interrupt(1, interrupt_controller);
        }
        self.schedule_sector_read(scheduler);
    }

    /// Handle a CdRomResponse event from the scheduler, pushing the response bytes to the FIFO and raising the
    /// interrupt
    pub fn handle_response_event(
        &mut self,
        bytes: Vec<u8>,
        int_code: u8,
        scheduler: &mut Scheduler,
        interrupt_controller: &mut InterruptController,
    ) {
        if self.interrupt_flags != 0 {
            // Previous IRQ not acknowledged, so we need to retry this response later
            scheduler.schedule(
                SchedulerEvent::CdRomResponse { bytes, int_code },
                cdrom_timing::RETRY_CYCLES,
            );
            return;
        }

        self.push_response(&bytes);
        self.set_interrupt(int_code, interrupt_controller);
    }
}

// Register handling
impl CdRom {
    pub fn read_register(&mut self, offset: u32) -> u8 {
        match offset {
            0 => self.read_status(),
            1 => self.read_result(),
            2 => self.read_data_fifo(),
            3 => self.read_interrupt_register(),
            _ => unreachable!("CD-ROM read from invalid offset {:02X}", offset),
        }
    }

    pub fn write_register(&mut self, offset: u32, value: u8, scheduler: &mut Scheduler) {
        match offset {
            0 => self.write_offset_0(value),
            1 => self.write_offset_1(value, scheduler),
            2 => self.write_offset_2(value),
            3 => self.write_offset_3(value),
            _ => unreachable!("CD-ROM write to invalid offset {:02X}", offset),
        }
    }

    // Address 1F801800h (offset 0)
    // Read = get status, write = set register index
    fn read_status(&self) -> u8 {
        /*
        1F801800h - Index/Status Register (Bit0-1 R/W) (Bit2-7 Read Only)
        0-1 Index   Port 1F801801h-1F801803h index (0..3 = Index0..Index3)   (R/W)
        2   ADPBUSY XA-ADPCM fifo empty  (0=Empty) ;set when playing XA-ADPCM sound
        3   PRMEMPT Parameter fifo empty (1=Empty) ;triggered before writing 1st byte
        4   PRMWRDY Parameter fifo full  (0=Full)  ;triggered after writing 16 bytes
        5   RSLRRDY Response fifo empty  (0=Empty) ;triggered after reading LAST byte
        6   DRQSTS  Data fifo empty      (0=Empty) ;triggered after reading LAST byte
        7   BUSYSTS Command/parameter transmission busy  (1=Busy)
        */

        let mut status = self.register_index & 0x03; // Bits 0-1: Index

        // Bit 3: parameter FIFO empty (PRMEMPT)
        if self.command_args.is_empty() {
            status |= 1 << 3; // Set bit 3 if parameter FIFO is empty
        }

        // Bit 4: parameter FIFO full (PRMWRDY) (0 = full), so we set it if we have less than 16 bytes
        if self.command_args.len() < 16 {
            status |= 1 << 4; // Set bit 4 if parameter FIFO is not full
        }

        // Bit 5: response FIFO empty (RSLRRDY) (0 = empty), so we set it if we have response bytes
        if !self.command_result.is_empty() {
            status |= 1 << 5; // Set bit 5 if response FIFO is empty
        }

        // Bit 6: data FIFO empty (DRQSTS) (0 = empty), so we set it if the sector buffer is ready
        if self.data_fifo_loaded {
            status |= 1 << 6; // Set bit 6 if data FIFO is not empty
        }

        // bit 7: busy status (BUSYSTS) (1 = busy), just false for now cause synchronous
        status
    }

    fn write_offset_0(&mut self, value: u8) {
        self.register_index = value & 0x03; // Only the lower 2 bits are valid for the index
    }

    // Address 1F801801h (offset 1)
    // Read = Response FIFO for all indices
    // Write =
    //          Index 0: Command register to send commands
    //          Index 1: Sound map data out
    //          Index 2: Sound map coding info
    //          Index 3: Audio Volume for Right-CD-Out to Right-SPU-Input
    fn read_result(&mut self) -> u8 {
        if let Some(byte) = self.command_result.pop_front() {
            byte
        } else {
            0 // Return 0 if the result queue is empty
        }
    }

    fn write_command_byte(&mut self, value: u8, scheduler: &mut Scheduler) {
        self.execute_command(value, scheduler);
    }

    fn write_offset_1(&mut self, value: u8, scheduler: &mut Scheduler) {
        match self.register_index {
            0 => self.write_command_byte(value, scheduler),
            1 => log!("Sound map data out set to {:02X}", value),
            2 => log!("Sound map coding info set to {:02X}", value),
            3 => self.cd_vol_rr = value, // Right-CD-Out to Right-SPU-Input
            _ => unreachable!(),
        }
    }

    // Address 1F801802h (offset 2)
    // Read = Data FIFO for all indices
    // Write =
    //          Index 0: CD Parameter Fifo
    //          Index 1: CD Interrupt Enable Register
    //          Index 2: CD Audio Volume for Left-CD-Out to Left-SPU-Input
    //          Index 3: CD Audio Volume for Right-CD-Out to Left-SPU-Input
    pub fn read_data_fifo(&mut self) -> u8 {
        // First, we need to see if the data FIFO even has anything
        if !self.data_fifo_loaded || self.data_buffer.is_empty() {
            // log!("Loaded = {}, Buffer empty = {:?}", self.data_fifo_loaded, self.data_buffer.is_empty());
            return 0; // Software really shouldn't reach here if it followed protocol
        }

        // Now padding: if we read outside the sector offset, we need to repeat the final byte of the sector buffer
        let pad_index = self.data_buffer.len() - if self.data_buffer.len() == 2340 { 4 } else { 8 };

        // Finally we can return the position at our cursor and move it, and also handle padding
        let byte = if self.data_buffer_offset < self.data_buffer.len() {
            self.data_buffer[self.data_buffer_offset]
        } else {
            self.data_buffer[pad_index] // Repeat the last byte of the sector buffer
        };

        self.data_buffer_offset += 1;

        // Are we done? Reset the data buffer if so
        if self.data_buffer_offset >= self.data_buffer.len() {
            self.data_fifo_loaded = false;
            self.data_buffer.clear();
        }

        byte
    }

    pub fn pull_cd_audio_sample(&mut self) -> (PcmSample, PcmSample) {
        let (raw_l, raw_r) =
            if let Some(sample) = self.xadpcm_decoder.pending_xa_samples.pop_front() {
                sample
            } else if let Some(sample) = self.pending_cdda_samples.pop_front() {
                sample
            } else {
                (PcmSample(0), PcmSample(0)) // buffer underrun - silence
            };

        let out_l = ((raw_l.0 as i32 * self.applied_vol_ll as i32
            + raw_r.0 as i32 * self.applied_vol_rl as i32)
            >> 7)
            .clamp(-0x8000, 0x7FFF) as i16;
        let out_r = ((raw_l.0 as i32 * self.applied_vol_lr as i32
            + raw_r.0 as i32 * self.applied_vol_rr as i32)
            >> 7)
            .clamp(-0x8000, 0x7FFF) as i16;

        (PcmSample(out_l), PcmSample(out_r))
    }

    fn write_parameter_fifo(&mut self, value: u8) {
        if self.command_args.len() < 16 {
            self.command_args.push(value);
        } else {
            elog!(
                "Warning: Parameter FIFO overflow, ignoring value {:02X}",
                value
            );
        }
    }

    fn write_interrupt_enable(&mut self, value: u8) {
        self.interrupt_enable = value & 0x1F; // Only the lower 5 bits are valid for interrupt enable
    }

    fn write_offset_2(&mut self, value: u8) {
        match self.register_index {
            0 => self.write_parameter_fifo(value),
            1 => self.write_interrupt_enable(value),
            2 => self.cd_vol_ll = value, // Left-CD-Out to Left-SPU-Input
            3 => self.cd_vol_lr = value, // Right-CD-Out to Left-SPU-Input
            _ => unreachable!(),
        }
    }

    // Address 1F801803h (offset 3)
    // Read =
    //          Index 0/2: CD interrupt enable register
    //          Index 1/3: CD interrupt flag register
    // Write =
    //          Index 0: CD Request Register
    //          Index 1: CD Interrupt Flag Register
    //          Index 2: CD Audio Volume for Left-CD-Out to Right-SPU-Input (W)
    //          Index 3: CD Audio Volume Apply Changes (by writing bit5=1)
    fn read_interrupt_register(&self) -> u8 {
        match self.register_index {
            0 | 2 => self.interrupt_enable,
            1 | 3 => self.interrupt_flags | 0xE0,
            _ => unreachable!(),
        }
    }

    fn write_request_register(&mut self, value: u8) {
        /*
        1F801803h.Index0 - Request Register (W)
        0-4 0    Not used (should be zero)
        5   SMEN Want Command Start Interrupt on Next Command (0=No change, 1=Yes)
        6   BFWR ...
        7   BFRD Want Data         (0=No/Reset Data Fifo, 1=Yes/Load Data Fifo)
        */

        if value & (1 << 5) != 0 {
            elog!("Request Register: Want Command Start Interrupt on Next Command");
        }

        // Weird behaviour from this bug
        // Basically, if the entire buffer was consumed, ONLY then do we reset the cursor position when setting BFRD
        let previously_loaded = self.data_fifo_loaded;
        self.data_fifo_loaded = value & (1 << 7) != 0;

        if self.data_fifo_loaded && !previously_loaded {
            self.data_buffer_offset = 0; // Reset the cursor position if we are loading the data FIFO
        }
    }

    fn write_interrupt_flag_register(&mut self, value: u8) {
        // Ack interrupts and commit second response if there is one if bit 0-2 is set
        // TODO all interrupts can be acked with 0x07?
        if value & 0x07 != 0 {
            self.interrupt_flags &= !(value & 0x07);
        }

        // Bit 6: reset parameter fifo (CLRPRM)
        if value & 0x40 != 0 {
            self.command_args.clear();
        }
    }

    fn write_offset_3(&mut self, value: u8) {
        match self.register_index {
            0 => self.write_request_register(value),
            1 => self.write_interrupt_flag_register(value),
            2 => self.cd_vol_rr = value, // Right-CD-Out to Right-SPU-Input
            3 => {
                // Check if bit5 = 1 and apply all volume changes if so
                if value & (1 << 5) != 0 {
                    self.applied_vol_ll = self.cd_vol_ll;
                    self.applied_vol_lr = self.cd_vol_lr;
                    self.applied_vol_rr = self.cd_vol_rr;
                    self.applied_vol_rl = self.cd_vol_rl;
                }
            }
            _ => unreachable!(),
        }
    }
}

// Disc reading
impl CdRom {
    fn schedule_sector_read(&mut self, scheduler: &mut Scheduler) {
        // Bit 7 of mode indicates double speed
        let delay: u64 = if self.mode & 0x80 != 0 {
            cdrom_timing::READ_INT1_2X
        } else {
            cdrom_timing::READ_INT1_1X
        };

        scheduler.schedule(SchedulerEvent::CdRomSectorRead, delay);
    }

    // Quick helper for sector read brancing
    fn track_type_for_lba(&self, lba: u32) -> TrackType {
        let Some(disc) = &self.disc else {
            return TrackType::Data;
        };
        disc.tracks
            .iter()
            .rev()
            .find(|t| t.start_logical_block <= lba)
            .map(|t| t.track_type.clone())
            .unwrap_or(TrackType::Data)
    }

    fn xa_filter_matches(&self, subheader: &XaSubHeader) -> bool {
        // If Setmode enabled the XA filter, only accept sectors matching the configured file/channel; otherwise accept
        // any audio sector.
        if !self.xa_filter_enabled {
            return true;
        }
        subheader.file_number == self.xa_filter_file
            && subheader.channel_number == self.xa_filter_channel
    }

    fn maybe_deliver_play_report(&mut self, lba: u32, scheduler: &mut Scheduler) {
        // Only send reports if Setmode bit 2 is enabled and we're playing
        if !self.playing || self.mode & 0x04 == 0 {
            return;
        }

        let disc_lba_with_leadin = lba + 150; // disc-absolute, includes 2s lead-in
        let asect = (disc_lba_with_leadin % 75) as u8;

        // Only fires on these 8 boundaries per second
        let report_absolute = match asect {
            0x00 | 0x20 | 0x40 | 0x60 => Some(true),
            0x10 | 0x30 | 0x50 | 0x70 => Some(false),
            _ => None,
        };

        let Some(report_absolute) = report_absolute else {
            return;
        };

        let Some(disc) = &self.disc else { return };
        let Some(track) = disc
            .tracks
            .iter()
            .rev()
            .find(|t| t.start_logical_block <= lba)
        else {
            return;
        };

        let track_number: u8 = track.track_number;
        let index = 1u8;

        let (peaklo, peakhi) = (0u8, 0u8); // TODO peak level

        let bytes = if report_absolute {
            let (amm, ass, _) = lba_to_msf_bcd(lba);
            let asect_bcd = decimal_to_bcd(asect);
            vec![
                self.get_status_byte(),
                decimal_to_bcd(track_number),
                decimal_to_bcd(index),
                amm,
                ass,
                asect_bcd,
                peaklo,
                peakhi,
            ]
        } else {
            let rel_lba = lba - track.start_logical_block;
            let (mm, ss, _) = lba_to_msf_bcd(rel_lba - 150); // -150 since track relative
            let sect = decimal_to_bcd((rel_lba % 75) as u8);
            vec![
                self.get_status_byte(),
                decimal_to_bcd(track_number),
                decimal_to_bcd(index),
                mm,
                ss | 0x80,
                sect,
                peaklo,
                peakhi,
            ]
        };

        self.schedule_event(cdrom_timing::DEFAULT_FIRST, bytes, 1, scheduler);
    }

    /// Performs a sector read and returns True if an interrupt should be raised for the sector
    fn perform_sector_read(&mut self, scheduler: &mut Scheduler) -> bool {
        self.sector_buffer = self.read_sector(self.current_logical_block);
        let track_type = self.track_type_for_lba(self.current_logical_block);
        self.current_logical_block += 1;

        if track_type == TrackType::Audio {
            self.extract_cdda_audio();
            self.maybe_deliver_play_report(self.current_logical_block - 1, scheduler);
            return false;
        }

        /*
        try_deliver_as_adpcm_sector:
        reject if CD-DA AUDIO format
        reject if sector isn't MODE2 format
        reject if adpcm_disabled(setmode.6)
        reject if filter_enabled(setmode.3) AND selected file/channel doesn't match
        reject if submode isn't audio+realtime (bit2 and bit6 must be both set)
        deliver: send sector to xa-adpcm decoder when passing above cases
        try_deliver_as_data_sector:
        reject data-delivery if "try_deliver_as_adpcm_sector" did do adpcm-delivery
        reject if filter_enabled(setmode.3) AND submode is audio+realtime (bit2+bit6)
        1st delivery attempt: send INT1+data, unless there's another INT pending
        delay, and retry at later time... but this time with file/channel checking!
        reject if filter_enabled(setmode.3) AND selected file/channel doesn't match
        2nd delivery attempt: send INT1+data, unless there's another INT pending
        */

        let subheader: XaSubHeader = XaSubHeader::parse(&self.sector_buffer);

        let adpcm_enabled = self.mode & 0x40 != 0; // Setmode bit 6
        let is_realtime_audio = subheader.is_audio() && subheader.is_realtime(); // bits 2 AND 6 of submode

        let deliver_as_adpcm = adpcm_enabled
            && is_realtime_audio
            && (!self.xa_filter_enabled || self.xa_filter_matches(&subheader));

        if deliver_as_adpcm {
            self.xadpcm_decoder
                .decode_sector(&subheader, &self.sector_buffer);
            return false;
        } else {
            let reject_data = self.xa_filter_enabled && is_realtime_audio;
            if !reject_data {
                self.extract_data_buffer();
            }
            return true;
        }
    }

    fn read_sector(&mut self, lba: u32) -> [u8; SECTOR_SIZE] {
        // Read the whole sector regardless
        let mut buf = [0u8; SECTOR_SIZE];
        let Some(disc) = &mut self.disc else {
            return buf;
        };

        // Find the track that contains the requested LBA, searching in reverse order to find the last track that
        // starts before or at the LBA
        let track = disc
            .tracks
            .iter()
            .rev()
            .find(|t| t.start_logical_block <= lba);

        let Some(track) = track else {
            return buf;
        };

        // Now we find the sector offset within the track and calculate the byte offset in the file
        let sector_offset_in_track = (lba - track.start_logical_block) as u64;
        let byte_offset = track.file_offset + sector_offset_in_track * SECTOR_SIZE as u64;

        // And perform the read from the file
        let file = &mut disc.files[track.file_index];
        file.read_at(byte_offset, &mut buf);

        buf
    }

    fn extract_cdda_audio(&mut self) {
        let samples: Vec<(PcmSample, PcmSample)> = self
            .sector_buffer
            .chunks_exact(4)
            .map(|c| {
                let l = i16::from_le_bytes([c[0], c[1]]);
                let r = i16::from_le_bytes([c[2], c[3]]);
                (PcmSample(l), PcmSample(r))
            })
            .collect();
        self.pending_cdda_samples.extend(samples);
    }

    fn extract_data_buffer(&mut self) {
        // Mode bit5 (0x20): 0 = 2048-byte data mode, 1 = 2340-byte whole-sector mode
        let (start, size) = if self.mode & 0x20 != 0 {
            (12, 2340) // Skip sync only
        } else {
            (24, 2048) // Skip sync + header + subheader
        };

        self.data_buffer = self.sector_buffer[start..start + size].to_vec();
        self.data_buffer_offset = 0;
    }
}

// Command execution
impl CdRom {
    // Dispatch
    fn execute_command(&mut self, cmd: u8, scheduler: &mut Scheduler) {
        // Clear response fifo and any pending interrupts before executing the command
        self.command_result.clear();
        scheduler.cancel(|event| matches!(event, SchedulerEvent::CdRomResponse { .. }));

        match cmd {
            0x01 => self.cmd_getstat(scheduler),
            0x02 => self.cmd_setloc(scheduler),
            0x03 => self.cmd_play(scheduler),
            0x06 => self.cmd_readn(scheduler),
            0x08 => self.cmd_stop(scheduler),
            0x09 => self.cmd_pause(scheduler),
            0x0A => self.cmd_init(scheduler),
            0x0C => self.cmd_demute(scheduler),
            0x0D => self.cmd_setfilter(scheduler),
            0x0E => self.cmd_setmode(scheduler),
            0x11 => self.cmd_getlocp(scheduler),
            0x13 => self.cmd_gettn(scheduler),
            0x14 => self.cmd_gettd(scheduler),
            0x15 => self.cmd_seekl(scheduler),
            0x16 => self.cmd_seekp(scheduler),
            0x19 => self.cmd_test(scheduler),
            0x1A => self.cmd_getid(scheduler),
            0x1B => self.cmd_reads(scheduler),
            _ => elog!("Unrecognized CD-ROM command 0x{:02X}", cmd),
        }
        self.command_args.clear(); // Clear the parameter FIFO after executing the command
    }

    // 0x01
    fn cmd_getstat(&mut self, scheduler: &mut Scheduler) {
        // Raise INT3 and send the status byte in the response FIFO
        let status_byte = self.get_status_byte();
        let response = [status_byte];
        self.schedule_event(cdrom_timing::GETSTAT, response.to_vec(), 3, scheduler);
    }

    // 0x02
    fn cmd_setloc(&mut self, scheduler: &mut Scheduler) {
        /*
        Setloc - Command 02h,amm,ass,asect --> INT3(stat)
        Sets the seek target - but without yet starting the seek operation. The actual seek is invoked by certain
        commands: SeekL (Data) and SeekP (Audio) are doing plain seeks (and do Pause after completion). ReadN/ReadS
        are similar to SeekL (and do start reading data after the seek operation). Play is similar to SeekP (and does
        start playing audio after the seek operation).
        The amm,ass,asect parameters refer to the entire disk (not to the current track). To seek to a specific
        location within a specific track, use GetTD to get the start address of the track, and add the desired time
        offset to it.
        */

        // Convert args from BCD to decimal
        let mm = bcd_to_decimal(self.command_args.get(0).copied().unwrap_or(0));
        let ss = bcd_to_decimal(self.command_args.get(1).copied().unwrap_or(0));
        let sect = bcd_to_decimal(self.command_args.get(2).copied().unwrap_or(0));

        self.reading = false;
        self.playing = false;
        // Cancel any pending sector read events since we are seeking to a new location
        scheduler.cancel(|event| matches!(event, SchedulerEvent::CdRomSectorRead));

        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![self.get_status_byte()],
            3,
            scheduler,
        );

        // 2 seconds of lead in on PS1 cd-roms supposedly... annoying.
        let lba = msf_to_lba(mm, ss, sect) - 150;
        self.seek_target = Some(lba);
    }

    // 0x03
    fn cmd_play(&mut self, scheduler: &mut Scheduler) {
        /*
        Play - Command 03h (,track) --> INT3(stat) --> optional INT1(report bytes)
        Starts CD Audio Playback. The parameter is optional, if there's no parameter given (or if it is 00h), then play
        either starts at Setloc position (if there was a pending unprocessed Setloc), or otherwise starts at the current
        location (eg. the last point seeked, or the current location of the current song; if it was already playing).
        For a disk with N songs, Parameters 1..N are starting the selected track. Parameters N+1..99h are restarting the
        begin of current track. The motor is switched off automatically when Play reaches the end of the disk, and
        INT4(stat) is generated (with stat.bit7 cleared).
        */

        // If seeking, schedule with SEEK_DELAY otherwise with DEFAULT_FIRST
        let seek_time = if self.seek_target.is_some() {
            cdrom_timing::SEEK_DELAY
        } else {
            cdrom_timing::DEFAULT_FIRST
        };

        self.schedule_event(seek_time, vec![self.get_status_byte()], 3, scheduler);

        let pending_seek = self.seek_target.take();

        self.reading = false;
        self.playing = true;

        // Play uses the current location when the parameter is omitted or 00h.
        let track_param = self.command_args.get(0).copied().unwrap_or(0);
        if track_param == 0 {
            if let Some(target) = pending_seek {
                self.current_logical_block = target;
            }
        } else if let Some(disc) = &self.disc {
            let track_dec = bcd_to_decimal(track_param);

            if let Some(track) = disc.tracks.iter().find(|t| t.track_number == track_dec) {
                self.current_logical_block = track.start_logical_block;
            } else if let Some(current_track) = disc
                .tracks
                .iter()
                .rev()
                .find(|t| t.start_logical_block <= self.current_logical_block)
            {
                // N+1..99h: restart current track
                self.current_logical_block = current_track.start_logical_block;
            }
        }

        scheduler.cancel(|event| matches!(event, SchedulerEvent::CdRomSectorRead));
        self.schedule_sector_read(scheduler);
    }

    // 0x06
    fn cmd_readn(&mut self, scheduler: &mut Scheduler) {
        /*
        ReadN - Command 06h --> INT3(stat) --> INT1(stat) --> datablock
        Read with retry. The command responds once with "stat,INT3", and then it's repeatedly sending
        "stat,INT1 --> datablock", that is continued even after a successful read has occured; use the Pause command to
        terminate the repeated INT1 responses.
        */

        let seek_time = if self.seek_target.is_some() {
            cdrom_timing::SEEK_DELAY
        } else {
            cdrom_timing::DEFAULT_FIRST
        };

        self.schedule_event(seek_time, vec![self.get_status_byte()], 3, scheduler);

        // Seek to our target if we have one first, no need to call seekl
        if let Some(target) = self.seek_target.take() {
            self.current_logical_block = target;
        }

        self.playing = false;
        self.reading = true;
        self.schedule_sector_read(scheduler);
    }

    // 0x09
    fn cmd_stop(&mut self, scheduler: &mut Scheduler) {
        /*
        Stop - Command 08h --> INT3(stat) --> INT2(stat)
        Stops motor with magnetic brakes (stops within a second or so) (unlike power-off where it'd keep spinning for
        about 10 seconds), and moves the drive head to the begin of the first track. Official way to restart is
        command 0Ah, but almost any command will restart it.
        The first response returns the current status (this already with bit5 cleared), the second response returns
        the new status (with bit1 cleared).
        */

        let stop_time = if self.reading || self.playing || self.seek_target.is_some() {
            if self.mode & 0x80 != 0 {
                cdrom_timing::STOP_SECOND_2X
            } else {
                cdrom_timing::STOP_SECOND_1X
            }
        } else {
            cdrom_timing::STOP_SECOND_ALREADY_STOPPED
        };

        // Should give the current status before stopping but with bit5 already cleared
        self.reading = false;
        self.playing = false;
        let status_before_stop = self.get_status_byte();

        self.seek_target = None;

        // Cancel any pending sector read events
        scheduler.cancel(|event| matches!(event, SchedulerEvent::CdRomSectorRead));

        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![status_before_stop],
            3,
            scheduler,
        );
        self.schedule_event(stop_time, vec![self.get_status_byte()], 2, scheduler);
    }

    // 0x09
    fn cmd_pause(&mut self, scheduler: &mut Scheduler) {
        /*
        Pause - Command 09h --> INT3(stat) --> INT2(stat)
        Aborts Reading and Playing, the motor is kept spinning, and the drive head maintains the current location
        within reasonable error.
        The first response returns the current status (still with bit5 set if a Read command was active), the second
        response returns the new status (with bit5 cleared).
        */

        let mut pause_time = if self.mode & 0x80 != 0 {
            cdrom_timing::PAUSE_SECOND_2X
        } else {
            cdrom_timing::PAUSE_SECOND_1X
        };
        pause_time = if self.reading || self.playing {
            pause_time
        } else {
            cdrom_timing::PAUSE_SECOND_ALREADY_PAUSED
        };
        let status_while_reading = self.get_status_byte();
        self.reading = false;
        self.playing = false;

        // Cancel any pending sector read events
        scheduler.cancel(|event| matches!(event, SchedulerEvent::CdRomSectorRead));

        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![status_while_reading],
            3,
            scheduler,
        );

        self.schedule_event(pause_time, vec![self.get_status_byte()], 2, scheduler);
    }

    // 0x0A
    fn cmd_init(&mut self, scheduler: &mut Scheduler) {
        /*
        Init - Command 0Ah --> INT3(stat) --> INT2(stat)
        Multiple effects at once. Sets mode=20h, activates drive motor, Standby, abort all commands.
        */

        // TODO motor
        self.mode = 0x20; // Set mode to 20h
        self.playing = false;
        self.reading = false;
        self.seek_target = None;

        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![self.get_status_byte()],
            3,
            scheduler,
        );
        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![self.get_status_byte()],
            2,
            scheduler,
        );
    }

    // 0x0C
    fn cmd_demute(&mut self, scheduler: &mut Scheduler) {
        /*
        Demute - Command 0Ch --> INT3(stat)
        Turn on audio streaming to SPU (affects both CD-DA and XA-ADPCM). The Demute command is needed only if one has
        formerly used the Mute command (by default, the PSX is demuted after power-up (...and/or after Init command?),
        and is demuted after cdrom-booting).
        */

        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![self.get_status_byte()],
            3,
            scheduler,
        );
    }

    // 0x0D
    fn cmd_setfilter(&mut self, scheduler: &mut Scheduler) {
        /*
        Setfilter - Command 0Dh,file,channel --> INT3(stat)
        Automatic ADPCM (CD-ROM XA) filter ignores sectors except those which have the same channel and file numbers in
        their subheader. This is the mechanism used to select which of multiple songs in a single .XA file to play.
        Setfilter does not affect actual reading (sector reads still occur for all sectors).
        XXX err... that is... does not affect reading of non-ADPCM sectors (normal "data" sectors are kept received
        regardless of Setfilter).
        */

        self.xa_filter_file = self.command_args.get(0).copied().unwrap_or(0);
        self.xa_filter_channel = self.command_args.get(1).copied().unwrap_or(0);

        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![self.get_status_byte()],
            3,
            scheduler,
        );
    }

    // 0x0E
    fn cmd_setmode(&mut self, scheduler: &mut Scheduler) {
        /*
        Setmode - Command 0Eh,mode --> INT3(stat)
        7   Speed       (0=Normal speed, 1=Double speed)
        6   XA-ADPCM    (0=Off, 1=Send XA-ADPCM sectors to SPU Audio Input)
        5   Sector Size (0=800h=DataOnly, 1=924h=WholeSectorExceptSyncBytes)
        4   Ignore Bit  (0=Normal, 1=Ignore Sector Size and Setloc position)
        3   XA-Filter   (0=Off, 1=Process only XA-ADPCM sectors that match Setfilter)
        2   Report      (0=Off, 1=Enable Report-Interrupts for Audio Play)
        1   AutoPause   (0=Off, 1=Auto Pause upon End of Track) ;for Audio Play
        0   CDDA        (0=Off, 1=Allow to Read CD-DA Sectors; ignore missing EDC)
        */

        self.mode = self.command_args.get(0).copied().unwrap_or(0);

        // Set some flags based on the mode bits
        // TODO: should mode be split up more? for example make double speed a bool
        self.xa_filter_enabled = self.mode & 0x08 != 0;

        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![self.get_status_byte()],
            3,
            scheduler,
        );
    }

    // 0x11
    fn cmd_getlocp(&mut self, scheduler: &mut Scheduler) {
        /*
        GetlocP - Command 11h - INT3(track,index,mm,ss,sect,amm,ass,asect)

        Retrieves 8 bytes of position information from Subchannel Q with ADR=1. Mainly intended for displaying the
        current audio position during Play. All results are in BCD.

        track:  track number (AAh=Lead-out area) (FFh=unknown, toc, none?)
        index:  index number (Usually 01h)
        mm:     minute number within track (00h and up)
        ss:     second number within track (00h to 59h)
        sect:   sector number within track (00h to 74h)
        amm:    minute number on entire disk (00h and up)
        ass:    second number on entire disk (00h to 59h)
        asect:  sector number on entire disk (00h to 74h)
        */

        let lba = self.current_logical_block;

        let Some(disc) = &self.disc else {
            // No disc, report unknown track/index and zero position
            self.schedule_event(
                cdrom_timing::DEFAULT_FIRST,
                vec![0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                3,
                scheduler,
            );
            return;
        };

        let track = disc
            .tracks
            .iter()
            .rev()
            .find(|t| t.start_logical_block <= lba);

        let Some(track) = track else {
            // Before the first track / unknown position
            self.schedule_event(
                cdrom_timing::DEFAULT_FIRST,
                vec![0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                3,
                scheduler,
            );
            return;
        };

        let track_number = track.track_number;
        let index = 1u8;

        let rel_lba = lba - track.start_logical_block;
        let (mm, ss, sect) = lba_to_msf_bcd(rel_lba);

        let disc_lba_with_leadin_removed = lba;
        let (amm, ass, asect) = lba_to_msf_bcd(disc_lba_with_leadin_removed);

        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![
                decimal_to_bcd(track_number),
                decimal_to_bcd(index),
                mm,
                ss,
                sect,
                amm,
                ass,
                asect,
            ],
            3,
            scheduler,
        );
    }

    // 0x13
    fn cmd_gettn(&mut self, scheduler: &mut Scheduler) {
        /*
        GetTN - Command 13h --> INT3(stat,first,last) ;BCD
        Get first track number, and last track number in the TOC of the current Session. The number of tracks in the
        current session can be calculated as (last-first+1). The first track number is usually 01h in the first
        (or only) session, and "last track of previous session plus 1" in further sessions.
        */

        let (first_track, last_track) = if let Some(disc) = &self.disc {
            let first = disc.tracks.first().map(|t| t.track_number).unwrap_or(1);
            let last = disc.tracks.last().map(|t| t.track_number).unwrap_or(1);
            (first, last)
        } else {
            (1, 1)
        };

        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![
                self.get_status_byte(),
                decimal_to_bcd(first_track),
                decimal_to_bcd(last_track),
            ],
            3,
            scheduler,
        );
    }

    // 0x14
    fn cmd_gettd(&mut self, scheduler: &mut Scheduler) {
        /*
        GetTD - Command 14h,track --> INT3(stat,mm,ss) ;BCD
        For a disk with NN tracks, parameter values 01h..NNh return the start of the specified track, parameter value
        00h returns the end of the last track, and parameter values bigger than NNh return error code 10h. Non-BCD
        parameter values also return error code 10h.
        The GetTD values are relative to Index=1 and are rounded down to second boundaries (eg. if track=N Index=0
        starts at 12:34:56, and Track=N Index=1 starts at 12:36:56, then GetTD(N) will return 12:36, ie. the sector
        number is truncated, and the Index=0 region is skipped).
        */

        let track_bcd = self.command_args.get(0).copied().unwrap_or(0);
        let track_dec = bcd_to_decimal(track_bcd);

        let Some(disc) = &self.disc else {
            self.schedule_event(
                cdrom_timing::DEFAULT_FIRST,
                vec![self.get_status_byte(), 0x10],
                5,
                scheduler,
            );
            return;
        };

        // Reject non-BCD values
        if decimal_to_bcd(track_dec) != track_bcd {
            self.schedule_event(
                cdrom_timing::DEFAULT_FIRST,
                vec![self.get_status_byte(), 0x10],
                5,
                scheduler,
            );
            return;
        }

        // Track 00h = end of last track
        let lba = if track_dec == 0 {
            let Some(last_track) = disc.tracks.last() else {
                self.schedule_event(
                    cdrom_timing::DEFAULT_FIRST,
                    vec![self.get_status_byte(), 0x10],
                    5,
                    scheduler,
                );
                return;
            };

            // Calculate end of disk
            let end_lba = if let Some(next_track) = disc
                .tracks
                .iter()
                .find(|t| t.start_logical_block > last_track.start_logical_block)
            {
                next_track.start_logical_block.saturating_sub(1)
            } else {
                let file = &disc.files[last_track.file_index];
                let file_size = file.len();
                let sector_count = (file_size / SECTOR_SIZE as u64) as u32;
                last_track
                    .start_logical_block
                    .saturating_add(sector_count.saturating_sub(1))
            };

            end_lba
        } else {
            let Some(track) = disc.tracks.iter().find(|t| t.track_number == track_dec) else {
                self.schedule_event(
                    cdrom_timing::DEFAULT_FIRST,
                    vec![self.get_status_byte(), 0x10],
                    5,
                    scheduler,
                );
                return;
            };
            track.start_logical_block
        };

        let (mm, ss, _) = lba_to_msf_bcd(lba);

        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![self.get_status_byte(), mm, ss],
            3,
            scheduler,
        );
    }

    // 0x15
    fn cmd_seekl(&mut self, scheduler: &mut Scheduler) {
        /*
        SeekL - Command 15h --> INT3(stat) --> INT2(stat)
        Seek to Setloc's location in data mode (using data sector header position data, which works/exists only on Data
        tracks, not on CD-DA Audio tracks). After the seek, the disk stays on the seeked location forever (namely: when
        seeking sector N, it does stay at around N-8..N-0 in single speed mode, or at around N-5..N+2 in double speed
        mode).
         */
        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![self.get_status_byte()],
            3,
            scheduler,
        );

        // Consume the seek target and update our current location
        if let Some(target) = self.seek_target.take() {
            self.current_logical_block = target;
        }

        // Second response: INt2(Stat)
        self.schedule_event(
            cdrom_timing::SEEK_DELAY,
            vec![self.get_status_byte()],
            2,
            scheduler,
        );
    }

    // 0x16
    fn cmd_seekp(&mut self, scheduler: &mut Scheduler) {
        /*
        SeekP - Command 16h --> INT3(stat) --> INT2(stat)
        Seek to Setloc's location in audio mode (using the Subchannel Q position data, which works on both Audio on
        Data disks).
        After the seek, the disk stays on the seeked location forever (namely: when seeking sector N, it does stay at
        around N-9..N-1 in single speed mode, or at around N-2..N in double speed mode). This command will stop any
        current or pending ReadN or ReadS.
        Note: Some older docs claim that SeekP would recurse only "MM:SS" of the "MM:SS:FF" position from Setloc -
        that is wrong, it does seek to MM:SS:FF (verified on a PSone).
        After the seek, status is stat.bit7=0 (ie. audio playback off), until sending a new Play command
        (without parameters) to start playback at the seeked location.
        */

        self.schedule_event(
            cdrom_timing::DEFAULT_FIRST,
            vec![self.get_status_byte()],
            3,
            scheduler,
        );

        // Stop any current or pending ReadN/ReadS, per spec
        self.reading = false;
        scheduler.cancel(|event| matches!(event, SchedulerEvent::CdRomSectorRead));

        // Consume the seek target and update our current location
        if let Some(target) = self.seek_target.take() {
            self.current_logical_block = target;
        }

        // Audio playback is off after a seek, until a fresh Play command starts it
        self.playing = false;

        // Second response: INT2(stat)
        self.schedule_event(
            cdrom_timing::SEEK_DELAY,
            vec![self.get_status_byte()],
            2,
            scheduler,
        );
    }

    // 0x19
    fn cmd_test(&mut self, scheduler: &mut Scheduler) {
        // First parameter byte contains the subcommand, only 0x20 (version) used by the BIOS
        let subcommand = self.command_args.get(0).copied().unwrap_or(0);
        match subcommand {
            0x20 => {
                // Get cdrom BIOS date/version (yy,mm,dd,ver) and set INT3
                let response = [0x95, 0x05, 0x16, 0xc1]; // Example response for BIOS version
                self.schedule_event(cdrom_timing::DEFAULT_FIRST, response.to_vec(), 3, scheduler);
            }
            _ => {
                elog!("Unrecognized CD-ROM TEST subcommand 0x{:02X}", subcommand);
                self.schedule_event(cdrom_timing::DEFAULT_FIRST, vec![0xFF], 3, scheduler); // Default response
            }
        }
    }

    // 0x1A
    fn cmd_getid(&mut self, scheduler: &mut Scheduler) {
        /*
        GetID - Command 1Ah --> INT3(stat) --> INT2/5 (stat,flags,type,atip,"SCEx")
        Drive Status           1st Response   2nd Response
        Door Open              INT5(11h,80h)  N/A
        Spin-up                INT5(01h,80h)  N/A
        Detect busy            INT5(03h,80h)  N/A
        No Disk                INT3(stat)     INT5(08h,40h, 00h,00h, 00h,00h,00h,00h)
        Audio Disk             INT3(stat)     INT5(0Ah,90h, 00h,00h, 00h,00h,00h,00h)
        Unlicensed:Mode1       INT3(stat)     INT5(0Ah,80h, 00h,00h, 00h,00h,00h,00h)
        Unlicensed:Mode2       INT3(stat)     INT5(0Ah,80h, 20h,00h, 00h,00h,00h,00h)
        Unlicensed:Mode2+Audio INT3(stat)     INT5(0Ah,90h, 20h,00h, 00h,00h,00h,00h)
        Debug/Yaroze:Mode2     INT3(stat)     INT2(02h,00h, 20h,00h, 20h,20h,20h,20h)
        Licensed:Mode2         INT3(stat)     INT2(02h,00h, 20h,00h, 53h,43h,45h,4xh)
        Modchip:Audio/Mode1    INT3(stat)     INT2(02h,00h, 00h,00h, 53h,43h,45h,4xh)
        */
        // Start by acknowledging with INT3 with status byte
        self.schedule_event(
            cdrom_timing::GETSTAT,
            vec![self.get_status_byte()],
            3,
            scheduler,
        );

        // For now, only respond with No Disk or Licensed:Mode2, depending on whether a disc is inserted
        if self.disc.is_none() {
            // No disc inserted
            let response = [0x08, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
            self.schedule_event(cdrom_timing::GETID_SECOND, response.to_vec(), 5, scheduler); // INT5 for second response
        } else {
            // Disc inserted, respond with Licensed:Mode2
            let response = [0x02, 0x00, 0x20, 0x00, b'S', b'C', b'E', b'A'];
            self.schedule_event(cdrom_timing::GETID_SECOND, response.to_vec(), 2, scheduler); // INT2 for second response
        }
    }

    // 0x1B
    fn cmd_reads(&mut self, scheduler: &mut Scheduler) {
        /*
        ReadS - Command 1Bh --> INT3(stat) --> INT1(stat) --> datablock
        Read without automatic retry. Not sure what that means... does WHAT on errors? Maybe intended for continous
        streaming video output (to skip bad frames, rather than to interrupt the stream by performing read-retrys).
        */

        // TODO a retry method, for now we'll just ReadS
        self.cmd_readn(scheduler);
    }

    // Helpers
    fn push_response(&mut self, response: &[u8]) {
        for &byte in response {
            self.command_result.push_back(byte);
        }
    }

    fn set_interrupt(&mut self, int_code: u8, interrupt_controller: &mut InterruptController) {
        self.interrupt_flags |= int_code & 0x1F; // Set the interrupt flag bits

        // Check if the nth bit for the code is enabled and raise the interrupt if so
        // 1=indexed interrupts but 0-indexed bits, so need to subtract 1 from the code to get the correct bit position
        if self.interrupt_enable & (1 << (int_code - 1)) != 0 {
            interrupt_controller.raise_interrupt(Interrupt::CDROM);
        }
    }

    fn schedule_event(
        &mut self,
        cycles: u64,
        response: Vec<u8>,
        interrupt_code: u8,
        scheduler: &mut Scheduler,
    ) {
        scheduler.schedule(
            SchedulerEvent::CdRomResponse {
                bytes: response,
                int_code: interrupt_code,
            },
            cycles,
        );
    }

    fn get_status_byte(&self) -> u8 {
        let mut status = 0u8;
        status |= 1 << 1;

        // Either seeking, reading or none: cant do both! need to error that first
        if self.seek_target.is_some() {
            status |= 1 << 6; // Seeking
        } else if self.reading || self.playing {
            status |= 1 << 5; // Reading
        }

        if self.seek_target.is_some() && (self.reading || self.playing) {
            panic!("CD-ROM cannot be seeking and reading at the same time");
        }

        if self.disc.is_none() {
            status |= 1 << 4; // Shell open / no disc
        }
        status
    }
}

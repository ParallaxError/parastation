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
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use crate::interrupt_controller::{Interrupt, InterruptController};

const SECTOR_SIZE: usize = 2352; // Size of a CD-ROM sector in bytes

/// Inserted disc structure, represented by a file and a list of tracks.
struct Disc {
    file: Vec<File>,    // File handles for the disc image
    tracks: Vec<Track>, // List of tracks on the disc
}

#[derive(Clone)]
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

// TODO global scheduler would be way better

enum CdRomEventKind {
    Response(Vec<u8>),
    SectorRead,
}

/// Deferred CDROM response that fires when the number of cycles remaining reaches 0, and any previous response has
/// been acknowledged. The response is then delivered with the interrupt of the specified code and the given response
/// bytes.
struct CdRomEvent {
    cycles_remaining: u32,
    kind: CdRomEventKind,
    interrupt_code: u8,
}

// Response timings
// First response delays
mod cdrom_timing {
    pub const GETSTAT: u32 = 0x000c4e1;
    pub const GETSTAT_STOPPED: u32 = 0x0005cf4;
    pub const INIT_FIRST: u32 = 0x0013cce;
    pub const DEFAULT_FIRST: u32 = 0x000c4e1; // Fallback, need to test all commands

    // Second response delays
    pub const GETID_SECOND: u32 = 0x0004a00;
    pub const PAUSE_SECOND_1X: u32 = 0x021181c;
    pub const PAUSE_SECOND_2X: u32 = 0x010bd93;
    pub const PAUSE_SECOND_ALREADY_PAUSED: u32 = 0x0001df2;
    pub const STOP_SECOND_1X: u32 = 0x0d38aca;
    pub const STOP_SECOND_2X: u32 = 0x18a6076;
    pub const STOP_SECOND_ALREADY_STOPPED: u32 = 0x0001d7b;

    // INT1 rate (per-sector delay during ReadN/ReadS)
    pub const READ_INT1_1X: u32 = 0x006e1cd;
    pub const READ_INT1_2X: u32 = 0x0036cd2;
}

/// CD-ROM controller to handle commands and disk access.
/// Basedn mostly off of https://hitmen.c02.at/files/docs/psx/psx.pdf
pub struct CdRom {
    register_index: u8,    // Index of the currently selected register (0x1F801800)
    command_args: Vec<u8>, // Arguments for the current command
    command_result: VecDeque<u8>, // Result of the last command
    interrupt_flags: u8,   // Interrupt flags for the CD-ROM controller (lower nybble of CDREG3)
    interrupt_enable: u8,  // IRQ enable mask
    event_queue: VecDeque<CdRomEvent>, // Queue of deferred CD-ROM events
    mode: u8,              // Mode set by the Setmode command (0x0E)

    seek_target: Option<u32>, // Target logical block address for seek operations
    current_logical_block: u32, // Current logical block address of the disc head
    reading: bool,            // Currently reading sectors (ReadN/ReadS command)
    disc: Option<Disc>,       // Currently inserted disc, if any

    sector_buffer: [u8; SECTOR_SIZE], // Buffer for storing the current sector data, useful portion in data_buffer
    data_fifo_loaded: bool, // Set by the "want data" bit, indicates that sector reading FIFO has data
    data_buffer: Vec<u8>,   // Buffer for data FIFO reads
    data_buffer_offset: usize, // Read offset within the data buffer for FIFO reads
}

// Helpers

/// Convert from the MSF (Minutes:Seconds:Frames) format to LBA (Logical Block Addressing)
/// https://github.com/opsxcq/psx-cue-sbi-collection
fn msf_to_lba(minutes: u8, seconds: u8, frames: u8) -> u32 {
    (minutes as u32 * 60 + seconds as u32) * 75 + frames as u32
}

fn bcd_to_decimal(bcd: u8) -> u8 {
    ((bcd >> 4) * 10) + (bcd & 0x0F)
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
            event_queue: VecDeque::new(),
            mode: 0,

            seek_target: None,
            current_logical_block: 0,
            reading: false,
            disc: None,

            sector_buffer: [0u8; SECTOR_SIZE],
            data_fifo_loaded: false,
            data_buffer: Vec::new(),
            data_buffer_offset: 0,
        }
    }

    /// Read a CUE file from the given path and parse it into a Disc structure
    pub fn insert_disc(&mut self, path: &str) {
        // https://github.com/opsxcq/psx-cue-sbi-collection
        // First, acquire the CUE file and parse it to get the track information
        // CUE is pretty simple, commands giving .bin locations and track information
        let content = std::fs::read_to_string(path).expect("Failed to read CUE file");
        let cue_directory = std::path::Path::new(path).parent().unwrap();

        // Need to accumulate our file descriptors as we acquire them as well as tracks
        let mut files: Vec<File> = Vec::new();
        let mut tracks: Vec<Track> = Vec::new();

        let mut current_file_index: usize = 0;
        let mut current_track_number: u8 = 0;
        let mut current_track_type: TrackType = TrackType::Data;

        // Track the LBA where the current file begins so that we can calculate the file offset for each track
        let mut current_file_base_lba: u32 = 0;

        // Now we read tokens as space separated, and match FILEs, TRACKs, and INDEXes
        for line in content.lines() {
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
                        let file_size = prev_file.metadata().map(|m| m.len()).unwrap_or(0);
                        let sector_count = (file_size / SECTOR_SIZE as u64) as u32;
                        current_file_base_lba += sector_count;
                    }

                    let file_path = cue_directory.join(filename);
                    let file = File::open(file_path).expect("Failed to open disc image file");
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
                    } // only care about INDEX 01

                    let time_parts: Vec<&str> = tokens[2].split(':').collect();
                    let minutes: u8 = time_parts[0].parse().unwrap();
                    let seconds: u8 = time_parts[1].parse().unwrap();
                    let frames: u8 = time_parts[2].parse().unwrap();

                    let local_lba = msf_to_lba(minutes, seconds, frames);
                    let file_offset = local_lba as u64 * SECTOR_SIZE as u64;

                    let global_start_lba = current_file_base_lba + local_lba;

                    println!(
                        "Parsed track {}: start_lba={}, file_index={}, file_offset={}",
                        current_track_number, global_start_lba, current_file_index, file_offset
                    );

                    tracks.push(Track {
                        track_number: current_track_number,
                        start_logical_block: global_start_lba,
                        track_type: current_track_type.clone(),
                        file_index: current_file_index,
                        file_offset,
                    });
                }
                _ => {
                    println!("Unrecognized CUE command: {}", tokens[0]);
                }
            }
        }

        self.disc = Some(Disc {
            file: files,
            tracks,
        });
    }

    /// Emulate the CD-ROM execution for a given number of cycles
    pub fn tick(&mut self, cycles: u32, interrupt_controller: &mut InterruptController) {
        // Only the front event counts down, others wait
        // Also need interrupts to be acked in order to execute a pending event
        if let Some(event) = self.event_queue.front_mut() {
            if event.cycles_remaining <= cycles {
                event.cycles_remaining = 0;
            } else {
                event.cycles_remaining -= cycles;
            }

            if event.cycles_remaining == 0 && self.interrupt_flags == 0 {
                // Only deliver once previous IRQ has been acked (interrupt_flags cleared)
                let event = self.event_queue.pop_front().unwrap();
                match event.kind {
                    CdRomEventKind::Response(bytes) => self.push_response(&bytes),
                    CdRomEventKind::SectorRead => {
                        self.perform_sector_read();
                        self.push_response(&[self.get_status_byte()]);

                        // ReadN keeps going — schedule the NEXT sector now, if still reading
                        if self.reading {
                            self.schedule_sector_read();
                        }
                    }
                }
                self.set_interrupt(event.interrupt_code, interrupt_controller);
            }
        }
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

    pub fn write_register(&mut self, offset: u32, value: u8) {
        match offset {
            0 => self.write_offset_0(value),
            1 => self.write_offset_1(value),
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

    fn write_command_byte(&mut self, value: u8) {
        self.execute_command(value);
    }

    fn write_offset_1(&mut self, value: u8) {
        match self.register_index {
            0 => self.write_command_byte(value),
            1 => println!("Sound map data out set to {:02X}", value),
            2 => println!("Sound map coding info set to {:02X}", value),
            3 => println!(
                "Audio Volume for Right-CD-Out to Right-SPU-Input set to {:02X}",
                value
            ),
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
            // println!("Loaded = {}, Buffer empty = {:?}", self.data_fifo_loaded, self.data_buffer.is_empty());
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

    fn write_parameter_fifo(&mut self, value: u8) {
        if self.command_args.len() < 16 {
            self.command_args.push(value);
        } else {
            eprintln!(
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
            2 => println!(
                "CD Audio Volume for Left-CD-Out to Left-SPU-Input set to {:02X}",
                value
            ),
            3 => println!(
                "CD Audio Volume for Right-CD-Out to Left-SPU-Input set to {:02X}",
                value
            ),
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
            eprintln!("Request Register: Want Command Start Interrupt on Next Command");
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
            2 => println!(
                "CD Audio Volume for Left-CD-Out to Right-SPU-Input set to {:02X}",
                value
            ),
            3 => println!("CD Audio Volume Apply Changes set to {:02X}", value),
            _ => unreachable!(),
        }
    }
}

// Disc reading
impl CdRom {
    fn schedule_sector_read(&mut self) {
        // Bit 7 of mode indicates double speed
        let delay = if self.mode & 0x80 != 0 {
            cdrom_timing::READ_INT1_2X
        } else {
            cdrom_timing::READ_INT1_1X
        };
        self.event_queue.push_back(CdRomEvent {
            cycles_remaining: delay,
            kind: CdRomEventKind::SectorRead,
            interrupt_code: 1, // INT1 — DataReady
        });
    }

    fn perform_sector_read(&mut self) {
        self.sector_buffer = self.read_sector(self.current_logical_block);
        self.current_logical_block += 1;
        self.extract_data_buffer();
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
        let file = &mut disc.file[track.file_index];
        if file.seek(SeekFrom::Start(byte_offset)).is_ok() {
            let _ = file.read_exact(&mut buf);
        }

        buf
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
    fn execute_command(&mut self, cmd: u8) {
        match cmd {
            0x01 => self.cmd_getstat(),
            0x02 => self.cmd_setloc(),
            0x06 => self.cmd_readn(),
            0x09 => self.cmd_pause(),
            0x0A => self.cmd_init(),
            0x0C => self.cmd_demute(),
            0x0E => self.cmd_setmode(),
            0x15 => self.cmd_seekl(),
            0x19 => self.cmd_test(),
            0x1A => self.cmd_getid(),
            0x1B => self.cmd_reads(),
            _ => eprintln!("Unrecognized CD-ROM command 0x{:02X}", cmd),
        }
        self.command_args.clear(); // Clear the parameter FIFO after executing the command
    }

    // 0x01
    fn cmd_getstat(&mut self) {
        // Raise INT3 and send the status byte in the response FIFO
        let status_byte = self.get_status_byte();
        let response = [status_byte];
        self.schedule_event(cdrom_timing::GETSTAT, response.to_vec(), 3);
    }

    // 0x02
    fn cmd_setloc(&mut self) {
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

        self.schedule_event(cdrom_timing::DEFAULT_FIRST, vec![self.get_status_byte()], 3);

        // 2 seconds of lead in on PS1 cd-roms supposedly... annoying.
        let lba = msf_to_lba(mm, ss, sect) - 150;
        self.seek_target = Some(lba);
    }

    // 0x06
    fn cmd_readn(&mut self) {
        /*
        ReadN - Command 06h --> INT3(stat) --> INT1(stat) --> datablock
        Read with retry. The command responds once with "stat,INT3", and then it's repeatedly sending
        "stat,INT1 --> datablock", that is continued even after a successful read has occured; use the Pause command to
        terminate the repeated INT1 responses.
        */
        self.schedule_event(cdrom_timing::DEFAULT_FIRST, vec![self.get_status_byte()], 3);

        // Seek to our target if we have one first, no need to call seekl
        if let Some(target) = self.seek_target.take() {
            self.current_logical_block = target;
        }

        self.reading = true;
        self.schedule_sector_read();
    }

    // 0x09
    fn cmd_pause(&mut self) {
        /*
        Pause - Command 09h --> INT3(stat) --> INT2(stat)
        Aborts Reading and Playing, the motor is kept spinning, and the drive head maintains the current location
        within reasonable error.
        The first response returns the current status (still with bit5 set if a Read command was active), the second
        response returns the new status (with bit5 cleared).
        */

        // let status_while_reading = self.get_status_byte();
        let mut pause_time = if self.mode & 0x80 != 0 {
            cdrom_timing::PAUSE_SECOND_2X
        } else {
            cdrom_timing::PAUSE_SECOND_1X
        };
        pause_time = if self.reading {
            pause_time
        } else {
            cdrom_timing::PAUSE_SECOND_ALREADY_PAUSED
        };
        let status_while_reading = self.get_status_byte();
        self.reading = false;

        // Abort any already-scheduled sector reads — Pause stops reading immediately
        self.event_queue
            .retain(|event| !matches!(event.kind, CdRomEventKind::SectorRead));

        self.schedule_event(cdrom_timing::DEFAULT_FIRST, vec![status_while_reading], 3);
        self.schedule_event(pause_time, vec![self.get_status_byte()], 2);
    }

    // 0x0A
    fn cmd_init(&mut self) {
        /*
        Init - Command 0Ah --> INT3(stat) --> INT2(stat)
        Multiple effects at once. Sets mode=20h, activates drive motor, Standby, abort all commands.
        */

        // TODO motor
        self.mode = 0x20; // Set mode to 20h

        self.schedule_event(cdrom_timing::DEFAULT_FIRST, vec![self.get_status_byte()], 3);
        self.schedule_event(cdrom_timing::DEFAULT_FIRST, vec![self.get_status_byte()], 2);
    }

    // 0x0C
    fn cmd_demute(&mut self) {
        /*
        Demute - Command 0Ch --> INT3(stat)
        Turn on audio streaming to SPU (affects both CD-DA and XA-ADPCM). The Demute command is needed only if one has
        formerly used the Mute command (by default, the PSX is demuted after power-up (...and/or after Init command?),
        and is demuted after cdrom-booting).
        */

        self.schedule_event(cdrom_timing::DEFAULT_FIRST, vec![self.get_status_byte()], 3);
    }

    // 0x0E
    fn cmd_setmode(&mut self) {
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
        self.schedule_event(cdrom_timing::DEFAULT_FIRST, vec![self.get_status_byte()], 3);
    }

    // 0x15
    fn cmd_seekl(&mut self) {
        /*
        SeekL - Command 15h --> INT3(stat) --> INT2(stat)
        Seek to Setloc's location in data mode (using data sector header position data, which works/exists only on Data
        tracks, not on CD-DA Audio tracks). After the seek, the disk stays on the seeked location forever (namely: when
        seeking sector N, it does stay at around N-8..N-0 in single speed mode, or at around N-5..N+2 in double speed
        mode).
         */
        self.schedule_event(cdrom_timing::DEFAULT_FIRST, vec![self.get_status_byte()], 3);

        // Consume the seek target and update our current location
        if let Some(target) = self.seek_target.take() {
            self.current_logical_block = target;
        }

        // Second response: INt2(Stat)
        self.schedule_event(cdrom_timing::DEFAULT_FIRST, vec![self.get_status_byte()], 2);
    }

    // 0x19
    fn cmd_test(&mut self) {
        // First parameter byte contains the subcommand, only 0x20 (version) used by the BIOS
        let subcommand = self.command_args.get(0).copied().unwrap_or(0);
        match subcommand {
            0x20 => {
                // Get cdrom BIOS date/version (yy,mm,dd,ver) and set INT3
                let response = [0x95, 0x05, 0x16, 0xc1]; // Example response for BIOS version
                self.schedule_event(cdrom_timing::DEFAULT_FIRST, response.to_vec(), 3);
            }
            _ => {
                eprintln!("Unrecognized CD-ROM TEST subcommand 0x{:02X}", subcommand);
                self.schedule_event(cdrom_timing::DEFAULT_FIRST, vec![0xFF], 3); // Default response
            }
        }
    }

    // 0x1A
    fn cmd_getid(&mut self) {
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
        self.schedule_event(cdrom_timing::GETSTAT, vec![self.get_status_byte()], 3);

        // For now, only respond with No Disk or Licensed:Mode2, depending on whether a disc is inserted
        if self.disc.is_none() {
            // No disc inserted
            let response = [0x08, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
            self.schedule_event(cdrom_timing::GETID_SECOND, response.to_vec(), 5); // INT5 for second response
        } else {
            // Disc inserted, respond with Licensed:Mode2
            let response = [0x02, 0x00, 0x20, 0x00, b'S', b'C', b'E', b'I'];
            self.schedule_event(cdrom_timing::GETID_SECOND, response.to_vec(), 2); // INT2 for second response
        }
    }

    // 0x1B
    fn cmd_reads(&mut self) {
        /*
        ReadS - Command 1Bh --> INT3(stat) --> INT1(stat) --> datablock
        Read without automatic retry. Not sure what that means... does WHAT on errors? Maybe intended for continous
        streaming video output (to skip bad frames, rather than to interrupt the stream by performing read-retrys).
        */

        // TODO a retry method, for now we'll just ReadS
        self.cmd_readn();
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

    fn schedule_event(&mut self, cycles: u32, response: Vec<u8>, interrupt_code: u8) {
        self.event_queue.push_back(CdRomEvent {
            cycles_remaining: cycles,
            kind: CdRomEventKind::Response(response),
            interrupt_code,
        });
    }

    fn get_status_byte(&self) -> u8 {
        let mut status = 0u8;
        status |= 1 << 1;

        // Either seeking, reading or none: cant do both! need to error that first
        if self.seek_target.is_some() {
            status |= 1 << 6; // Seeking
        } else if self.reading {
            status |= 1 << 5; // Reading
        }

        if self.seek_target.is_some() && self.reading {
            panic!("CD-ROM cannot be seeking and reading at the same time");
        }

        if self.disc.is_none() {
            status |= 1 << 4; // Shell open / no disc
        }
        status
    }
}

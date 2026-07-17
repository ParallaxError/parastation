/*
 * @file /parastation-core/src/spu/voice.rs
 * @brief
 * One of the PS1's 24 voices, containing the state of the voice and providing an interface to the sound generation
 * components it owns (ADPCM decoder, envelope generation, etc.)
 *
 * https://jsgroth.dev/blog/posts/ps1-spu-part-1/#memory-map
 * https://psx-spx.consoledev.net/soundprocessingunitspu/#1f801c00hn10h-voice-023-volume-left onwards
 *
 * -----
 */

// Imports
use crate::spu::PcmSample;
use crate::spu::adpcm::AdpcmDecoder;
use crate::spu::envelope::{AdsrEnvelope, SweepEnvelope};
use crate::spu::interpolate::interpolate_gaussian;

// Loop flags encoded in a ADPCM block, to be decoded before playback
/*
0   Loop End    (0=No change, 1=Set ENDX flag and Jump to [1F801C0Eh+N*10h])
1   Loop Repeat (0=Force Release and set ADSR Level to Zero; only if Bit0=1)
2   Loop Start  (0=No change, 1=Copy current address to [1F801C0Eh+N*10h])
3-7 Unknown    (usually 0)
*/

#[derive(Debug, Clone, Copy)]
struct LoopFlags {
    end: bool,
    repeat: bool,
    start: bool,
}

impl LoopFlags {
    fn from_header_byte(byte: u8) -> Self {
        Self {
            end: byte & 0x01 != 0,
            repeat: byte & 0x02 != 0,
            start: byte & 0x04 != 0,
        }
    }
}

/// One of the PS1's 24 voices, containing the state of the voice and providing an interface to the sound generation
/// components it owns (ADPCM decoder, envelope generation, etc.)
#[derive(Debug, Clone, Copy)]
pub struct Voice {
    sample_rate: u16,    // $1F801C04 + N*$10: ADPCM sample rate
    start_address: u32,  // $1F801C06 + N*$10: ADPCM start address
    repeat_address: u32, // $1F801C0E + N*$10: ADPCM repeat address

    // Current playback state
    current_address: u32,
    pitch_counter: u16,
    decode_buffer: [PcmSample; 28], // 28 samples per ADPCM block
    current_buffer_idx: usize,      // Index into decode_buffer for the next sample to output
    recent_samples: [PcmSample; 4], // Previous 4 decoded samples for Gaussian interpolation
    current_amplitude: i16,

    // Owned sound generation components
    decoder: AdpcmDecoder,
    envelope: AdsrEnvelope,
    sweep_left: SweepEnvelope,
    sweep_right: SweepEnvelope,

    keyed_on: bool,
    endx: bool, // ENDX status bit, set on loop end and cleared on key on
}

impl Voice {
    pub fn new() -> Self {
        Self {
            sample_rate: 0,
            start_address: 0,
            repeat_address: 0,
            current_address: 0,
            pitch_counter: 0,
            decode_buffer: [PcmSample(0); 28],
            current_buffer_idx: 0,
            recent_samples: [PcmSample(0); 4],
            current_amplitude: 0,
            decoder: AdpcmDecoder::new(),
            envelope: AdsrEnvelope::new(),
            sweep_left: SweepEnvelope::new(),
            sweep_right: SweepEnvelope::new(),
            keyed_on: false,
            endx: false,
        }
    }

    // Register reads
    pub fn read_sample_rate(&self) -> u16 {
        self.sample_rate
    }

    pub fn read_start_address(&self) -> u16 {
        (self.start_address >> 3) as u16 // Convert back to 8 byte units
    }

    pub fn read_repeat_address(&self) -> u16 {
        (self.repeat_address >> 3) as u16 // Convert back to 8 byte units
    }

    pub fn read_current_level(&self) -> i16 {
        self.envelope.read_level()
    }

    pub fn read_adsr_config_low(&self) -> u16 {
        self.envelope.read_config_low()
    }

    pub fn read_adsr_config_high(&self) -> u16 {
        self.envelope.read_config_high()
    }

    pub fn endx(&self) -> bool {
        self.endx
    }

    pub fn keyed_on(&self) -> bool {
        self.keyed_on
    }

    // Register writes
    pub fn write_sample_rate(&mut self, value: u16) {
        self.sample_rate = value;
    }

    pub fn write_start_address(&mut self, value: u16) {
        /*
        15-0   Startaddress of sound in Sound buffer (in 8-byte units)
        */
        // 8 byte units so shift left by 3 to get the actual address
        self.start_address = (value as u32) << 3;
    }

    pub fn write_repeat_address(&mut self, value: u16) {
        /*
        15-0  Address sample loops to at end (in 8-byte units)
        */
        // 8 byte units so shift left by 3 to get the actual address
        self.repeat_address = (value as u32) << 3;
    }

    pub fn write_adsr_config_low(&mut self, config: u16) {
        self.envelope.write_config_low(config);
    }

    pub fn write_adsr_config_high(&mut self, config: u16) {
        self.envelope.write_config_high(config);
    }

    pub fn read_sweep_config_left(&self) -> u16 {
        self.sweep_left.read_config()
    }

    pub fn read_sweep_config_right(&self) -> u16 {
        self.sweep_right.read_config()
    }

    pub fn write_sweep_config_left(&mut self, config: u16) {
        self.sweep_left.write_config(config);
    }

    pub fn write_sweep_config_right(&mut self, config: u16) {
        self.sweep_right.write_config(config);
    }

    // Key on/off
    pub fn key_on(&mut self, sound_ram: &[u8]) {
        self.envelope.key_on();
        self.decoder = AdpcmDecoder::new(); // Reset prev/prev_prev sample history
        self.current_address = self.start_address;
        self.pitch_counter = 0;
        self.recent_samples = [PcmSample(0); 4];
        self.current_buffer_idx = 0;
        self.keyed_on = true;
        self.endx = false;
        self.decode_next_block(sound_ram); // Start decoding the next block to prepare for playback
    }

    pub fn key_off(&mut self) {
        self.keyed_on = false;
        self.envelope.key_off();
    }
}

// Audio decoding and playback
impl Voice {
    /// Decode the next ADPCM block from sound RAM and update the decode buffer and loop state
    /// Returns the address of the block so that IRQ can be triggered if needed
    fn decode_next_block(&mut self, sound_ram: &[u8]) -> Option<u32> {
        let addr = (self.current_address as usize) % 0x80000; // Wrap around sound RAM size
        let block: [u8; 16] = sound_ram[addr..addr + 16]
            .try_into()
            .expect("SPU RAM slice for ADPCM block must be exactly 16 bytes");

        self.decode_buffer = self.decoder.decode(block);

        let loop_flags = LoopFlags::from_header_byte(block[1]);

        if loop_flags.start {
            self.repeat_address = self.current_address;
        }

        if loop_flags.end {
            self.endx = true;
            self.current_address = self.repeat_address;
            if !loop_flags.repeat {
                self.envelope.force_end();
            }
        } else {
            self.current_address += 16;
        }

        Some(addr as u32)
    }

    // Slide the decode buffer and recent samples forward by one sample, called after each sample is output
    fn advance_sample(&mut self) {
        let new_sample = self.decode_buffer[self.current_buffer_idx as usize];
        self.recent_samples[0] = self.recent_samples[1];
        self.recent_samples[1] = self.recent_samples[2];
        self.recent_samples[2] = self.recent_samples[3];
        self.recent_samples[3] = new_sample;
    }

    /// Advance the voice's state by one tick, called every 768 CPU cycles
    /// Returns the raw current sample pre-envelope and interpolation, and the fetched ADPCM block address if a new
    /// block was fetched (for IRQ triggering)
    pub fn tick(
        &mut self,
        sound_ram: &[u8],
        pmon_enabled: bool,
        prev_amplitude: i16,
        noise_enabled: bool,
        noise_output: i16,
    ) -> (PcmSample, Option<u32>) {
        /*
        Step = VxPitch                  ;range +0000h..+FFFFh (0...705.6 kHz)
        IF PMON.Bit(x)=1 AND (x>0)      ;pitch modulation enable
            Factor = VxOUTX(x-1)          ;range -8000h..+7FFFh (prev voice amplitude)
            Factor = Factor+8000h         ;range +0000h..+FFFFh (factor = 0.00 .. 1.99)
            Step=SignExpand16to32(Step)   ;hardware glitch on VxPitch>7FFFh, make sign
            Step = (Step * Factor) SAR 15 ;range 0..1FFFFh (glitchy if VxPitch>7FFFh)
            Step=Step AND 0000FFFFh       ;hardware glitch on VxPitch>7FFFh, kill sign
        IF Step>3FFFh then Step=4000h   ;range +0000h..+3FFFh (0.. 176.4kHz)
        Counter = Counter + Step
        */

        let mut step = self.sample_rate as u32;

        if pmon_enabled {
            let factor = (prev_amplitude as i32 + 0x8000) as u32;
            let step_signed = self.sample_rate as i16 as i32;
            let modulated = (step_signed * factor as i32) >> 15;
            step = (modulated as u32) & 0xFFFF;
        }

        let step = step.min(0x4000) as u16;
        self.pitch_counter = self.pitch_counter.wrapping_add(step);

        let mut fetched_addr = None;
        while self.pitch_counter >= 0x1000 {
            self.pitch_counter -= 0x1000;
            self.current_buffer_idx += 1;
            if self.current_buffer_idx == 28 {
                self.current_buffer_idx = 0;
                fetched_addr = self.decode_next_block(sound_ram);
            }
            self.advance_sample();
        }

        let raw_sample = if !noise_enabled {
            interpolate_gaussian(&self.recent_samples, self.pitch_counter)
        } else {
            PcmSample(noise_output)
        };

        (raw_sample, fetched_addr)
    }

    pub fn current_envelope_volume(&mut self) -> i16 {
        let envelope_level = self.envelope.tick();

        envelope_level
    }

    pub fn current_sweep_volume_left(&mut self) -> i16 {
        let sweep_level = self.sweep_left.tick();

        sweep_level
    }

    pub fn current_sweep_volume_right(&mut self) -> i16 {
        let sweep_level = self.sweep_right.tick();

        sweep_level
    }

    pub fn set_current_amplitude(&mut self, amplitude: i32) {
        self.current_amplitude = amplitude.clamp(-0x8000, 0x7FFF) as i16;
    }

    pub fn current_amplitude(&self) -> i16 {
        self.current_amplitude
    }
}

/*
 * @file /parastation-core/src/xadpcm.rs
 * @brief
 * Implementation for the XA ADPCM decoder used in the PS1 CD-ROM controller.
 *
 * https://psx-spx.consoledev.net/cdromformat/#cdrom-xa-audio-adpcm-compression
 * LOT from https://github.com/jsgroth/CoffeePSX/blob/master/crates/ps1-core/src/cd/xaadpcm.rs
 * -----
 */

// Imports
use std::collections::VecDeque;

use crate::cd_rom::SECTOR_SIZE;
use crate::spu::PcmSample;

/// CD-ROM XA subheader (8 bytes following the 12 byte sync pattern and 4 byte sector header), as outlined in
/// https://psx-spx.consoledev.net/cdromformat/#cdrom-xa-subheader-file-channel-interleave
pub struct XaSubHeader {
    pub file_number: u8,
    pub channel_number: u8,
    pub submode: u8,     // Bit flags: audio, video, data, end-of-record, etc
    pub coding_info: u8, // sample rate, bits-per-sample, mono/stereo, emphasis
}

impl XaSubHeader {
    const SUBMODE_END_OF_RECORD: u8 = 0x01;
    const SUBMODE_AUDIO: u8 = 0x04;
    const SUBMODE_REALTIME: u8 = 0x40;

    const CODING_STEREO: u8 = 0x01;
    const CODING_HALF_RATE: u8 = 0x04; // 18900Hz instead of 37800Hz

    pub fn parse(sector_buffer: &[u8; SECTOR_SIZE]) -> Self {
        Self {
            file_number: sector_buffer[16],
            channel_number: sector_buffer[17],
            submode: sector_buffer[18],
            coding_info: sector_buffer[19],
        }
    }

    pub fn is_audio(&self) -> bool {
        self.submode & Self::SUBMODE_AUDIO != 0
    }

    pub fn is_realtime(&self) -> bool {
        self.submode & Self::SUBMODE_REALTIME != 0
    }

    pub fn is_end_of_record(&self) -> bool {
        self.submode & Self::SUBMODE_END_OF_RECORD != 0
    }

    fn is_stereo(&self) -> bool {
        self.coding_info & Self::CODING_STEREO != 0
    }

    fn is_half_rate(&self) -> bool {
        self.coding_info & Self::CODING_HALF_RATE != 0
    }
}

/// 7-phase, 29-tap Gaussian interpolation table used to resample XA-ADPCM's 18900Hz/37800Hz output up to the SPU's
/// fixed 44100Hz
/// https://github.com/jsgroth/CoffeePSX/blob/master/crates/ps1-core/src/cd/xaadpcm.rs

struct ResampleRingBuffer {
    samples: [i16; 32],
    write_idx: usize,
}

impl ResampleRingBuffer {
    fn new() -> Self {
        Self {
            samples: [0; 32],
            write_idx: 0,
        }
    }

    fn push(&mut self, value: i16) {
        self.samples[self.write_idx] = value;
        self.write_idx = (self.write_idx + 1) & 0x1F;
    }

    /// Reads the sample `offset` positions before the most recently pushed one.
    fn history(&self, offset: usize) -> i16 {
        self.samples[self.write_idx.wrapping_sub(offset) & 0x1F]
    }
}

const INTERPOLATION_TAPS: usize = 29;
const INTERPOLATION_PHASES: usize = 7;
pub const INTERPOLATION: &[i16; 7 * 29] = &[
    0, 0, 0, 0, -0x0001, 0x0002, -0x0005, 0, 0, 0, -0x0001, 0x0003, -0x0008, 0x0011, 0, 0, -0x0001,
    0x0003, -0x0008, 0x0010, -0x0023, 0, -0x0002, 0x0003, -0x0008, 0x0011, -0x0023, 0x0046, 0, 0,
    -0x0002, 0x0006, -0x0010, 0x002B, -0x0017, -0x0002, 0x0003, -0x0005, 0x0005, 0x000A, 0x001A,
    -0x0044, 0x000A, -0x0013, 0x001F, -0x001B, 0x006B, -0x00EB, 0x015B, -0x0022, 0x003C, -0x004A,
    0x00A6, -0x016D, 0x027B, -0x0347, 0x0041, -0x004B, 0x00B3, -0x01A8, 0x0350, -0x0548, 0x080E,
    -0x0054, 0x00A2, -0x0192, 0x0372, -0x0623, 0x0AFA, -0x1249, 0x0034, -0x00E3, 0x02B1, -0x05BF,
    0x0BCD, -0x16FA, 0x3C07, 0x0009, 0x0132, -0x039E, 0x09B8, -0x1780, 0x53E0, 0x53E0, -0x010A,
    -0x0043, 0x04F8, -0x11B4, 0x6794, 0x3C07, -0x16FA, 0x0400, -0x0267, -0x05A6, 0x74BB, 0x234C,
    -0x1249, 0x0AFA, -0x0A78, 0x0C9D, 0x7939, 0x0C9D, -0x0A78, 0x080E, -0x0548, 0x234C, 0x74BB,
    -0x05A6, -0x0267, 0x0400, -0x0347, 0x027B, 0x6794, -0x11B4, 0x04F8, -0x0043, -0x010A, 0x015B,
    -0x00EB, -0x1780, 0x09B8, -0x039E, 0x0132, 0x0009, -0x0044, 0x001A, 0x0BCD, -0x05BF, 0x02B1,
    -0x00E3, 0x0034, -0x0017, 0x002B, -0x0623, 0x0372, -0x0192, 0x00A2, -0x0054, 0x0046, -0x0023,
    0x0350, -0x01A8, 0x00B3, -0x004B, 0x0041, -0x0023, 0x0010, -0x016D, 0x00A6, -0x004A, 0x003C,
    -0x0022, 0x0011, -0x0008, 0x006B, -0x001B, 0x001F, -0x0013, 0x000A, -0x0005, 0x0002, 0x000A,
    0x0005, -0x0005, 0x0003, -0x0001, 0, 0, -0x0010, 0x0006, -0x0002, 0, 0, 0, 0, 0x0011, -0x0008,
    0x0003, -0x0002, 0x0001, 0, 0, -0x0008, 0x0003, -0x0001, 0, 0, 0, 0, 0x0003, -0x0001, 0, 0, 0,
    0, 0, -0x0001, 0, 0, 0, 0, 0, 0,
];

fn resample_to_44100hz(
    half_rate: bool,
    input: &[i16],
    output: &mut Vec<i16>,
    ring_buffer: &mut ResampleRingBuffer,
) {
    let pushes_per_sample = if half_rate { 2 } else { 1 };

    let mut phase_counter = 0;
    for &sample in input {
        for _ in 0..pushes_per_sample {
            ring_buffer.push(sample);

            phase_counter += 1;
            if phase_counter < 6 {
                continue;
            }
            phase_counter = 0;

            for phase in 0..INTERPOLATION_PHASES {
                let mut sum: i32 = 0;
                for tap in 1..INTERPOLATION_TAPS {
                    let history_sample: i32 = ring_buffer.history(tap).into();
                    let coeff: i32 = INTERPOLATION[INTERPOLATION_PHASES * (tap - 1) + phase].into();
                    sum += (history_sample * coeff) >> 15;
                }
                output.push(sum.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
            }
        }
    }
}

// K0 and K1 filters from https://psx-spx.consoledev.net/cdromformat/#xa-adpcm-header-bytes
const FILTER_K0: [i32; 4] = [0, 60, 115, 98];
const FILTER_K1: [i32; 4] = [0, 0, -52, -55];

// Number of 128-byte data blocks per XA sector
const DATA_BLOCKS_PER_SECTOR: usize = 18;
const DATA_BLOCK_SIZE: usize = 128;

// Byte offset of the first data block within the raw 2352 byte sector (12 byte sync + 4 byte header + 8 byte
// subheader)
const AUDIO_DATA_START: usize = 24;
const AUDIO_DATA_LEN: usize = DATA_BLOCKS_PER_SECTOR * DATA_BLOCK_SIZE;

fn sign_extend_nibble(byte: u8, high_nibble: bool) -> i32 {
    // Helper to sign extend a 4-bit ADPCM sample nibble to i32
    let nibble = if high_nibble { byte >> 4 } else { byte } & 0x0F;
    (((nibble as i8) << 4) >> 4) as i32
}

/// Two most recent decoded samples for one ADPCM prediction stream
#[derive(Clone, Copy, Default)]
struct AdpcmHistory {
    prev1: i32,
    prev2: i32,
}

impl AdpcmHistory {
    fn push(&mut self, sample: i16) {
        self.prev2 = self.prev1;
        self.prev1 = sample as i32;
    }
}

#[derive(Clone, Copy)]
enum ChannelSlot {
    Left,
    Right,
}

impl ChannelSlot {
    fn index(self) -> usize {
        match self {
            ChannelSlot::Left => 0,
            ChannelSlot::Right => 1,
        }
    }
}

/// XADPCM decoder for the PS1 CD-ROM controller, used for decoding XA audio tracks. Tracks the previously decoded
/// samples for interpolation and just provides one function to decode a single XA sector into PCM samples
/// ADPCM prediction history and resampling ring buffers persist across calls for consistent audio output throughout a
/// stream
pub struct XadpcmDecoder {
    pub pending_xa_samples: VecDeque<(PcmSample, PcmSample)>,

    history_l: AdpcmHistory,
    history_r: AdpcmHistory,
    decoded_units: [Vec<i16>; 2],

    resample_ring_l: ResampleRingBuffer,
    resample_ring_r: ResampleRingBuffer,
}

impl XadpcmDecoder {
    pub fn new() -> Self {
        Self {
            pending_xa_samples: VecDeque::new(),
            history_l: AdpcmHistory::default(),
            history_r: AdpcmHistory::default(),
            decoded_units: [Vec::new(), Vec::new()],
            resample_ring_l: ResampleRingBuffer::new(),
            resample_ring_r: ResampleRingBuffer::new(),
        }
    }

    /// Decode one raw 2352 byte XA sector and queue the resulting samples onto the internal pending_xa_samples buffer
    pub fn decode_sector(&mut self, subheader: &XaSubHeader, sector_buffer: &[u8; SECTOR_SIZE]) {
        let stereo = subheader.is_stereo();
        let half_rate = subheader.is_half_rate();

        let audio_data = &sector_buffer[AUDIO_DATA_START..AUDIO_DATA_START + AUDIO_DATA_LEN];

        for data_block in audio_data.chunks_exact(DATA_BLOCK_SIZE) {
            for pair in 0..4 {
                if stereo {
                    self.decode_audio_block(data_block, 2 * pair, ChannelSlot::Left);
                    self.decode_audio_block(data_block, 2 * pair + 1, ChannelSlot::Right);
                } else {
                    // Mono streams both sub-blocks into left slot
                    self.decode_audio_block(data_block, 2 * pair, ChannelSlot::Left);
                    self.decode_audio_block(data_block, 2 * pair + 1, ChannelSlot::Left);
                }
            }
        }

        let decoded_l = std::mem::take(&mut self.decoded_units[ChannelSlot::Left.index()]);
        let decoded_r = if stereo {
            std::mem::take(&mut self.decoded_units[ChannelSlot::Right.index()])
        } else {
            decoded_l.clone()
        };

        let mut resampled_l = Vec::new();
        let mut resampled_r = Vec::new();
        resample_to_44100hz(
            half_rate,
            &decoded_l,
            &mut resampled_l,
            &mut self.resample_ring_l,
        );
        resample_to_44100hz(
            half_rate,
            &decoded_r,
            &mut resampled_r,
            &mut self.resample_ring_r,
        );

        let sample_count = resampled_l.len().min(resampled_r.len());
        for i in 0..sample_count {
            self.pending_xa_samples
                .push_back((PcmSample(resampled_l[i]), PcmSample(resampled_r[i])));
        }
    }

    /// Decode one 28 sample audio block into "slot"'s prediction history
    /// Uses the same algorithm as spu/adpcm.rs, resources are attached there
    fn decode_audio_block(&mut self, data_block: &[u8], block_idx: usize, slot: ChannelSlot) {
        let header_byte = data_block[0x04 + block_idx];
        let shift = header_byte & 0x0F;
        let filter = ((header_byte >> 4) & 0x03) as usize;

        let effective_shift = 12i32.saturating_sub(shift as i32).clamp(0, 31);
        let k0 = FILTER_K0[filter];
        let k1 = FILTER_K1[filter];

        let history = match slot {
            ChannelSlot::Left => &mut self.history_l,
            ChannelSlot::Right => &mut self.history_r,
        };
        let mut prediction = *history;

        let decoded_unit = &mut self.decoded_units[slot.index()];

        for sample_idx in 0..28usize {
            let sample_byte = data_block[16 + 4 * sample_idx + block_idx / 2];
            let sample = sign_extend_nibble(sample_byte, block_idx % 2 == 1);

            let shifted = sample << effective_shift;
            let predicted = shifted + (k0 * prediction.prev1 + k1 * prediction.prev2 + 32) / 64;
            let clamped = predicted.clamp(i16::MIN as i32, i16::MAX as i32) as i16;

            prediction.push(clamped);
            decoded_unit.push(clamped);
        }

        *history = prediction;
    }
}

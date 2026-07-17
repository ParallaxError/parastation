/*
 * @file /parastation-core/src/spu/adpcm.rs
 * @brief
 * ADPCM decoder for the PS1 SPU.
 *
 * ADPCM samples consist of 16 bytes containing 28 samples.
 * Starts with a 2 byte header and 14 bytes of 4 bit samples, and this module decodes them into 16 bit PCM samples.
 *
 * Useful reference at https://jsgroth.dev/blog/posts/ps1-spu-part-1/, explaining the formula for decoding
 * https://psx-spx.consoledev.net/cdromformat/#xa-adpcm-header-bytes for actual header format
 * -----
 */

// Imports
use crate::spu::PcmSample;

/// The ADPCM decoder for the PS1 SPU, which uses the previous two samples to decode the next sample comprising the
/// entire state of the decoder
#[derive(Debug, Clone, Copy)]
pub struct AdpcmDecoder {
    prev_sample: PcmSample,
    prev_prev_sample: PcmSample,
}

impl AdpcmDecoder {
    pub fn new() -> Self {
        Self {
            prev_sample: PcmSample(0),
            prev_prev_sample: PcmSample(0),
        }
    }

    // Two filter tables from PSX-SPX
    const POS_XA_ADPCM_TABLE: [i16; 5] = [0, 60, 115, 98, 122];
    const NEG_XA_ADPCM_TABLE: [i16; 5] = [0, 0, -52, -55, -60];

    // Helper for converting a 4 bit signed value to a signed 32 bit value
    fn signed4bit(value: u8) -> i32 {
        let value = value & 0x0F; // Ensure only the lower 4 bits are used
        if value & 0x08 != 0 {
            (value as i32) - 16 // Convert to negative value
        } else {
            value as i32 // Positive value
        }
    }

    /// Decode a 16 byte ADPCM block into 28 16 bit PCM samples
    pub fn decode(&mut self, block: [u8; 16]) -> [PcmSample; 28] {
        // https://psx-spx.consoledev.net/soundprocessingunitspu/#sample-data-spu-adpcm
        /*
        Sample Data (SPU-ADPCM)
        Samples consist of one or more 16-byte blocks:

        00h       Shift/Filter (reportedly same as for CD-XA) (see there)
        01h       Flag Bits (see below)
        02h       Compressed Data (LSBs=1st Sample, MSBs=2nd Sample)
        03h       Compressed Data (LSBs=3rd Sample, MSBs=4th Sample)
        04h       Compressed Data (LSBs=5th Sample, MSBs=6th Sample)
        ...       ...
        0Fh       Compressed Data (LSBs=27th Sample, MSBs=28th Sample)
        */

        // First byte is the shift/filter byte, which contains the shift amount and filter index
        /*
        From psx-spx:
        shift  = 12 - (src[4+blk*2+nibble] AND 0Fh)
        filter =      (src[4+blk*2+nibble] AND 30h) SHR 4
        */
        let shift = 12 - (block[0] & 0x0F);
        let filter = (block[0] & 0x30) >> 4;

        // Finally the decoding logic just follows the psx-spx pseudocode
        /*
        f0 = pos_xa_adpcm_table[filter]
        f1 = neg_xa_adpcm_table[filter]
        for j=0 to 27
            t = signed4bit((src[16+blk+j*4] SHR (nibble*4)) AND 0Fh)
            s = (t SHL shift) + ((old*f0 + older*f1+32)/64);
            s = MinMax(s,-8000h,+7FFFh)
            halfword[dst]=s, dst=dst+2, older=old, old=s
        next j
        */

        // Initialise return array with zeroes
        let mut pcm_samples = [PcmSample(0); 28];

        let f0 = Self::POS_XA_ADPCM_TABLE[filter as usize];
        let f1 = Self::NEG_XA_ADPCM_TABLE[filter as usize];

        for j in 0..28 {
            let nibble = j % 2;
            let byte_index = 2 + (j / 2);
            let t = Self::signed4bit((block[byte_index] >> (nibble * 4)) & 0x0F);

            let s = (t << shift)
                + ((self.prev_sample.0 as i32 * f0 as i32
                    + self.prev_prev_sample.0 as i32 * f1 as i32
                    + 32)
                    / 64);
            let s_clamped = s.clamp(-32768, 32767) as i16;

            // Update the previous samples for the next iteration
            self.prev_prev_sample = self.prev_sample;
            self.prev_sample = PcmSample(s_clamped);

            // Write our clamped sample to the output array
            pcm_samples[j] = self.prev_sample;
        }

        pcm_samples
    }
}

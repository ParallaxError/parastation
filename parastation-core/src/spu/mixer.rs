/*
 * @file /parastation-core/src/spu/mixer.rs
 * @brief
 * Mixer for combining audio signals from multiple voices in the PS1 SPU.
 *
 * Only held state is the main output volume for the PS1, apart from that the state of every voice is passed in to the
 * main mixing function to be combined into a single (left, right) output signal.
 * -----
 */

// Imports
use crate::spu::PcmSample;

pub struct Mixer {
    main_volume_left: i16,
    main_volume_right: i16,
}

impl Mixer {
    pub fn new() -> Self {
        Self {
            main_volume_left: 0x7FFF, // TODO should this be the default? bios will probably overwrite anyway...
            main_volume_right: 0x7FFF,
        }
    }

    pub fn read_main_volume_left(&self) -> i16 {
        self.main_volume_left
    }

    pub fn read_main_volume_right(&self) -> i16 {
        self.main_volume_right
    }

    pub fn write_main_volume_left(&mut self, value: i16) {
        self.main_volume_left = value;
    }

    pub fn write_main_volume_right(&mut self, value: i16) {
        self.main_volume_right = value;
    }

    pub fn mix(
        &self,
        voice_samples: &[PcmSample; 24],
        voice_envelope_volumes: &[i16; 24],
        voice_volume_left: &[i16; 24],
        voice_volume_right: &[i16; 24],
    ) -> (PcmSample, PcmSample) {
        let mut left_sum: i32 = 0;
        let mut right_sum: i32 = 0;

        for i in 0..24 {
            let post_envelope =
                (voice_samples[i].0 as i32 * voice_envelope_volumes[i] as i32) >> 15;

            left_sum += (post_envelope * voice_volume_left[i] as i32) >> 15;
            right_sum += (post_envelope * voice_volume_right[i] as i32) >> 15;
        }

        (
            PcmSample(left_sum.clamp(-0x8000, 0x7FFF) as i16),
            PcmSample(right_sum.clamp(-0x8000, 0x7FFF) as i16),
        )
    }
}

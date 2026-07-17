/*
 * @file /parastation-frontend/src/wav_dump_backend.rs
 * @brief
 * Debug SpuBackend that buffers all samples in memory and writes a WAV file on drop.
 * Bypasses real-time playback timing entirely - useful for isolating DSP correctness bugs
 * from audio delivery/pacing bugs.
 *
 * ENTIRELY VIBE CODED! Slightly ashamed. Also realise I don't care because this is just for debugging
 */

use std::fs::File;
use std::io::{BufWriter, Write};

use parastation_core::spu::{PcmSample, SpuBackend};

pub struct WavDumpBackend {
    samples: Vec<(i16, i16)>,
    output_path: String,
}

impl WavDumpBackend {
    pub fn new(output_path: &str) -> Self {
        Self {
            samples: Vec::new(),
            output_path: output_path.to_string(),
        }
    }

    fn write_wav(&self) -> std::io::Result<()> {
        let file = File::create(&self.output_path)?;
        let mut writer = BufWriter::new(file);

        let num_samples = self.samples.len() as u32;
        let sample_rate: u32 = 44100;
        let num_channels: u16 = 2;
        let bits_per_sample: u16 = 16;
        let byte_rate = sample_rate * u32::from(num_channels) * u32::from(bits_per_sample) / 8;
        let block_align = num_channels * bits_per_sample / 8;
        let data_size = num_samples * u32::from(block_align);

        // RIFF header
        writer.write_all(b"RIFF")?;
        writer.write_all(&(36 + data_size).to_le_bytes())?;
        writer.write_all(b"WAVE")?;

        // fmt chunk
        writer.write_all(b"fmt ")?;
        writer.write_all(&16u32.to_le_bytes())?; // fmt chunk size
        writer.write_all(&1u16.to_le_bytes())?; // PCM format
        writer.write_all(&num_channels.to_le_bytes())?;
        writer.write_all(&sample_rate.to_le_bytes())?;
        writer.write_all(&byte_rate.to_le_bytes())?;
        writer.write_all(&block_align.to_le_bytes())?;
        writer.write_all(&bits_per_sample.to_le_bytes())?;

        // data chunk
        writer.write_all(b"data")?;
        writer.write_all(&data_size.to_le_bytes())?;
        for (l, r) in &self.samples {
            writer.write_all(&l.to_le_bytes())?;
            writer.write_all(&r.to_le_bytes())?;
        }

        writer.flush()
    }
}

impl SpuBackend for WavDumpBackend {
    fn push_sample(&mut self, left: PcmSample, right: PcmSample) {
        self.samples.push((left.0, right.0));
    }

    fn buffer_available(&self) -> usize {
        usize::MAX
    }
}

impl Drop for WavDumpBackend {
    fn drop(&mut self) {
        if let Err(e) = self.write_wav() {
            eprintln!("Failed to write WAV dump: {e}");
        } else {
            println!(
                "Wrote {} samples to {}",
                self.samples.len(),
                self.output_path
            );
        }
    }
}

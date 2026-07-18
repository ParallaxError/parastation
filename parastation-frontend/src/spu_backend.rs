/*
 * @file /parastation-frontend/src/spu_backend.rs
 * @brief
 * cpal based SpuBackend implementation for audio output. Uses a ring buffer to allow for some latency between the PS1's
 *  audio output and the host's audio output, to avoid glitches.
 *
 * Will admit, partially vibe coded. Not too interested in learning how cpal works in rust so meh
 */

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamConfig;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use parastation_core::spu::{PcmSample, SpuBackend};

const RING_BUFFER_CAPACITY: usize = 16384; // How many sample to buffer before dropping new samples to avoid glitches

struct SharedRingBuffer {
    left: VecDeque<i16>,
    right: VecDeque<i16>,
}

pub struct CpalSpuBackend {
    shared: Arc<Mutex<SharedRingBuffer>>,
    _stream: cpal::Stream, // Kept as member to keep the stream alive for the lifetime of the backend
}

impl CpalSpuBackend {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("No output audio device available!");

        let config = StreamConfig {
            channels: 2,
            sample_rate: 44100,
            buffer_size: cpal::BufferSize::Default,
        };

        let shared = Arc::new(Mutex::new(SharedRingBuffer {
            left: VecDeque::with_capacity(RING_BUFFER_CAPACITY),
            right: VecDeque::with_capacity(RING_BUFFER_CAPACITY),
        }));

        let stream_shared = Arc::clone(&shared);

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [i16], _info: &cpal::OutputCallbackInfo| {
                    let mut buf = stream_shared.lock().unwrap();
                    // data is interleaved [L, R, L, R, ...]
                    for frame in data.chunks_exact_mut(2) {
                        let l = buf.left.pop_front().unwrap_or(0);
                        let r = buf.right.pop_front().unwrap_or(0);
                        frame[0] = l;
                        frame[1] = r;
                    }
                },
                move |err| eprintln!("Cpal output stream error: {err}"),
                None,
            )
            .expect("Failed to build Cpal output stream");

        stream.play().expect("Failed to start Cpal stream");

        Self {
            shared,
            _stream: stream,
        }
    }
}

impl SpuBackend for CpalSpuBackend {
    fn push_sample(&mut self, left: PcmSample, right: PcmSample) {
        let mut buf = self.shared.lock().unwrap();
        if buf.left.len() < RING_BUFFER_CAPACITY {
            buf.left.push_back(left.0);
            buf.right.push_back(right.0);
        }
    }

    fn buffer_available(&self) -> usize {
        let buf = self.shared.lock().unwrap();
        RING_BUFFER_CAPACITY.saturating_sub(buf.left.len())
    }
}

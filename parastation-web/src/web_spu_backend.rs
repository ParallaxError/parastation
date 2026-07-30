/*
 * @file /parastation-web/src/web_spu_backend.rs
 * @brief
 * SpuBackend implementation for the web frontend. Buffers samples into a ring buffer that the JavaScript can drain
 * from through the WebRunner.
 *
 * -----
 */

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use parastation_core::spu::{PcmSample, SpuBackend};

const RING_BUFFER_CAPACITY: usize = 16384;

pub struct WebSpuBackend {
    // Left and right ring buffers
    left: VecDeque<i16>,
    right: VecDeque<i16>,
}

impl WebSpuBackend {
    pub fn new() -> Self {
        Self {
            left: VecDeque::with_capacity(RING_BUFFER_CAPACITY),
            right: VecDeque::with_capacity(RING_BUFFER_CAPACITY),
        }
    }

    fn push_sample(&mut self, left: i16, right: i16) {
        if self.left.len() < RING_BUFFER_CAPACITY {
            self.left.push_back(left);
            self.right.push_back(right);
        }
    }

    fn buffer_available(&self) -> usize {
        RING_BUFFER_CAPACITY.saturating_sub(self.left.len())
    }

    /// Drains up to `max_frames` samples as interleaved f32s (L, R, L, R, ...) from -1 to 1.
    pub fn drain_interleaved_f32(&mut self, max_frames: usize) -> Vec<f32> {
        let n = self.left.len().min(self.right.len()).min(max_frames);
        let mut out = Vec::with_capacity(n * 2);
        for _ in 0..n {
            let l = self.left.pop_front().unwrap_or(0);
            let r = self.right.pop_front().unwrap_or(0);
            out.push(l as f32 / 32768.0);
            out.push(r as f32 / 32768.0);
        }
        out
    }
}

/// Wrapper around a shared Rc<RefCell<WebSpuBackend>> to implement SpuBackend for the PS1 core
pub struct SharedSpuHandle(Rc<RefCell<WebSpuBackend>>);

impl SharedSpuHandle {
    pub fn new(shared: Rc<RefCell<WebSpuBackend>>) -> Self {
        Self(shared)
    }
}

impl SpuBackend for SharedSpuHandle {
    fn push_sample(&mut self, left: PcmSample, right: PcmSample) {
        self.0.borrow_mut().push_sample(left.0, right.0);
    }

    fn buffer_available(&self) -> usize {
        self.0.borrow().buffer_available()
    }
}

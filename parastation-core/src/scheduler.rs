/*
 * @file /parastation-core/src/scheduler.rs
 * @brief
 * Event scheduler using a metric for absolute time (like cycles elapsed) to schedule and dispatch events for
 * timing accurate peripherals like the CD-ROM.
 *
 * Owned by te SystemBus and passed as a mutable reference to components when they are allowed to schedule events.
 *
 * -----
 */

// Imports
use std::collections::BinaryHeap;

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum SchedulerEvent {
    VBlank,
    CdRomResponse { bytes: Vec<u8>, int_code: u8 },
    CdRomSectorRead,
    SioResponse { byte: u8, dsr: bool },
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ScheduledEvent {
    deadline: u64,
    kind: SchedulerEvent,
}

impl Ord for ScheduledEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse order so BinaryHeap (max-heap) gives us the smallest deadline first
        other.deadline.cmp(&self.deadline)
    }
}

impl PartialOrd for ScheduledEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub struct Scheduler {
    events: BinaryHeap<ScheduledEvent>,
    pub current_cycle: u64,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            events: BinaryHeap::new(),
            current_cycle: 0,
        }
    }

    /// Add a new event to the scheduler, to be dispatched in the given number of cycles from now
    pub fn schedule(&mut self, event: SchedulerEvent, cycles_from_now: u64) {
        let deadline = self.current_cycle + cycles_from_now;
        self.events.push(ScheduledEvent {
            deadline,
            kind: event,
        });
    }

    /// Get the time to the next scheduled event in cycles (u64::MAX if no events are scheduled)
    pub fn time_to_next_event(&self) -> u64 {
        if let Some(event) = self.events.peek() {
            event.deadline.saturating_sub(self.current_cycle)
        } else {
            u64::MAX
        }
    }

    /// Advance the scheduler by the given number of cycles and return an iterator of events that require dispatching
    pub fn advance(&mut self, cycles: u32) -> Vec<SchedulerEvent> {
        self.current_cycle += cycles as u64;
        let mut fired = Vec::new();
        while let Some(e) = self.events.peek() {
            if e.deadline <= self.current_cycle {
                fired.push(self.events.pop().unwrap().kind);
            } else {
                break;
            }
        }
        fired
    }

    /// Cancel any future events that satisfy the given predicate
    pub fn cancel(&mut self, predicate: impl Fn(&SchedulerEvent) -> bool) {
        // BinaryHeap doesn't support arbitrary removal, rebuild without matching events
        let events: Vec<_> = self.events.drain().collect();
        self.events
            .extend(events.into_iter().filter(|e| !predicate(&e.kind)));
    }
}

/*
 * @file /parastation-core/src/timer.rs
 * @brief
 * Implementation for the three timers available on the PS1, mapped at 0x1F80_1100.
 *
 * -----
 */

// Imports
use crate::elog;
use crate::interrupt_controller::{Interrupt, InterruptController};

#[derive(Clone, Copy, Default)]
struct RootCounter {
    current: u16,
    mode: u16,
    target: u16,
    irq_disabled_until_ack: bool, // Makes the ACK IRQ for one-shot mode level triggered instead of edge triggered
    clock_accumulator: u32,
    cached_divisor: u32,
}

impl RootCounter {
    fn new() -> Self {
        RootCounter {
            current: 0,
            mode: 0,
            target: 0,
            irq_disabled_until_ack: false,
            clock_accumulator: 0,
            cached_divisor: 1,
        }
    }

    fn read_current(&self) -> u16 {
        self.current
    }

    fn read_mode(&mut self) -> u16 {
        self.mode
    }

    fn read_target(&self) -> u16 {
        self.target
    }

    fn write_current(&mut self, value: u16) {
        self.current = value;
    }

    fn compute_divisor(mode: u16, timer_index: usize) -> u32 {
        let clock_source = (mode >> 8) & 0b11;
        match timer_index {
            1 if clock_source == 1 || clock_source == 3 => 2148,
            2 if clock_source == 2 || clock_source == 3 => 8,
            _ => 1,
        }
    }

    fn write_mode(&mut self, value: u16, timer_index: usize) {
        self.current = 0; // Reset the counter when the mode is written to
        self.clock_accumulator = 0;
        self.mode = value;
        self.cached_divisor = Self::compute_divisor(value, timer_index);
    }

    fn write_target(&mut self, value: u16) {
        self.target = value;
    }

    // Advance the counter by a given number of cycles, returning the new current value and whether the target or the
    // max value boundary was crossed
    #[inline(always)]
    fn advance_one_batch(
        &self,
        step: u16,
        target: u16,
        reset_on_target: bool,
    ) -> (u16, bool, bool) {
        let start = self.current as u32;
        let mut end = start + step as u32;

        let would_hit_target = (start..=end).contains(&(target as u32)) && target != 0;

        if would_hit_target && reset_on_target {
            end = target as u32;
        }

        let hit_target = would_hit_target;
        let hit_ffff = end > 0xFFFF;

        (end as u16, hit_target, hit_ffff)
    }

    #[inline(always)]
    fn tick(
        &mut self,
        cycles: u64,
        interrupt: Interrupt,
        interrupt_controller: &mut InterruptController,
    ) {
        // Check if we should send an IRQ when reaching target, max value, and reset on target
        let irq_on_target = self.mode & (1 << 4) != 0;
        let irq_on_ffff = self.mode & (1 << 5) != 0;
        let reset_on_target = self.mode & (1 << 3) != 0;

        // Clock divisor math for timers 1 and 2
        let divisor = self.cached_divisor as u64;
        let total = self.clock_accumulator as u64 + cycles;
        let effective_cycles = total / divisor;
        self.clock_accumulator = (total % divisor) as u32;

        if effective_cycles == 0 {
            return;
        }

        let target = self.target;
        let start = self.current as u64;
        let end = start + effective_cycles;

        let would_hit_target = target != 0 && start <= target as u64 && target as u64 <= end;
        let hit_ffff_fast = end > 0xFFFF;

        if !hit_ffff_fast && !(would_hit_target && reset_on_target) {
            self.current = end as u16;
            if would_hit_target && irq_on_target {
                interrupt_controller.raise_interrupt(interrupt);
            }
            return;
        }

        let mut remaining = effective_cycles;
        while remaining > 0 {
            let step = remaining.min(u16::MAX as u64);
            let (new_current, hit_target, hit_ffff) =
                self.advance_one_batch(step as u16, self.target, reset_on_target);
            self.current = new_current;

            let consumed = if hit_target && reset_on_target {
                (self.target as u32).saturating_sub(self.current as u32) as u64
            } else {
                step
            };

            remaining -= consumed.max(1);

            if hit_target && irq_on_target {
                interrupt_controller.raise_interrupt(interrupt);
            }
            if hit_ffff && irq_on_ffff {
                interrupt_controller.raise_interrupt(interrupt);
            }
            if hit_target && reset_on_target {
                self.current = 0;
            }
        }
    }
}

/// Three timers are available on the PS1, mapped at 0x1F80_1100
pub struct Timers {
    counters: [RootCounter; 3],
}

impl Timers {
    pub fn new() -> Self {
        Timers {
            counters: [RootCounter::new(), RootCounter::new(), RootCounter::new()],
        }
    }

    fn irq_for(timer: usize) -> Interrupt {
        match timer {
            0 => Interrupt::TMR0,
            1 => Interrupt::TMR1,
            2 => Interrupt::TMR2,
            _ => unreachable!(),
        }
    }

    pub fn read_register(&mut self, timer: usize, offset: u32) -> u32 {
        let counter = &mut self.counters[timer];
        match offset {
            0 => counter.read_current() as u32,
            4 => counter.read_mode() as u32,
            8 => counter.read_target() as u32,
            _ => {
                elog!("Read from invalid timer register offset {offset:#x} for timer {timer}");
                0
            }
        }
    }

    pub fn write_register(&mut self, timer: usize, offset: u32, value: u16) {
        let counter = &mut self.counters[timer];
        match offset {
            0 => counter.write_current(value),
            4 => counter.write_mode(value, timer),
            8 => counter.write_target(value),
            _ => elog!(
                "Write to invalid timer register offset {offset:#x} for timer {timer} with value {value:#x}"
            ),
        }
    }

    pub fn tick(&mut self, cycles: u64, ic: &mut InterruptController) {
        self.counters[0].tick(cycles, Interrupt::TMR0, ic);
        self.counters[1].tick(cycles, Interrupt::TMR1, ic);
        self.counters[2].tick(cycles, Interrupt::TMR2, ic);
    }
}

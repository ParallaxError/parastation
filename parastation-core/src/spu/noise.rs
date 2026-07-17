/*
 * @file /parastation-core/src/spu/noise.rs
 * @brief
 * Simple noise generator for the PS1 SPU, only one per SPU shared by all voices.
 *
 * https://psx-spx.consoledev.net/soundprocessingunitspu/#spu-noise-generator
 * -----
 */

pub struct NoiseGenerator {
    output: i16,
    step: u8,
    shift: u8,
    timer: i32,
}

impl NoiseGenerator {
    pub fn new() -> Self {
        Self {
            output: 0,
            step: 0,
            shift: 0,
            timer: 0,
        }
    }

    pub fn tick(&mut self) {
        /*
        Wait(1 cycle)          ;at 44.1kHz clock
        Timer=Timer-NoiseStep  ;subtract Step (4..7)
        ParityBit = NoiseLevel.Bit15 xor Bit12 xor Bit11 xor Bit10 xor 1
        IF Timer<0 then NoiseLevel = NoiseLevel*2 + ParityBit
        IF Timer<0 then Timer=Timer+(20000h SHR NoiseShift)  ;reload timer once
        IF Timer<0 then Timer=Timer+(20000h SHR NoiseShift)  ;reload again if needed
        */

        self.timer -= (self.step as i32) + 4;
        if self.timer >= 0 {
            return;
        }
        let bit = |v: i16, n: u32| ((v as u16) >> n) & 1 != 0;
        let parity = bit(self.output, 15)
            ^ bit(self.output, 12)
            ^ bit(self.output, 11)
            ^ bit(self.output, 10)
            ^ true;
        self.output = (self.output << 1) | (parity as i16);
        while self.timer < 0 {
            self.timer += 0x20000 >> self.shift;
        }
    }

    pub fn write_shift(&mut self, shift: u8) {
        self.shift = shift;
        self.timer = self.timer.min(0x20000 >> shift);
    }

    pub fn output(&self) -> i16 {
        self.output
    }
}

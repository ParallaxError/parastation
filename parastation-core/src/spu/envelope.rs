/*
 * @file /parastation-core/src/spu/envelope.rs
 * @brief
 * ADSR and Sweep state machines for envelope generation in the PS1 SPU.
 *
 * Provide functionality to interface with the generators state and obtain the current volume of the channel through
 * creating an envelope curve to model the volume of the channel over time.
 *
 * https://psx-spx.consoledev.net/soundprocessingunitspu/#envelope-operation-depending-on-shiftstepmodedirection
 * https://psx-spx.consoledev.net/soundprocessingunitspu/#1f801c00hn10h-voice-023-volume-left
 * https://jsgroth.dev/blog/posts/ps1-spu-part-2/.
 * -----
 */

// https://psx-spx.consoledev.net/soundprocessingunitspu/#1f801c02hn10h-voice-023-volume-right
/*
15    Must be set       (1=Sweep Mode)
14    Sweep Mode        (0=Linear, 1=Exponential)
13    Sweep Direction   (0=Increase, 1=Decrease)
12    Sweep Phase       (0=Positive, 1=Negative)
7-11  Not used?         (should be zero)
6-2   Sweep Shift       (0..1Fh = Fast..Slow)
1-0   Sweep Step        (0..3 = "+7,+6,+5,+4" or "-8,-7,-6,-5") (inc/dec)
*/
#[derive(Debug, Clone, Copy)]
pub struct SweepEnvelope {
    enabled: bool,
    mode_exponential: bool,
    direction_decrease: bool,
    phase_negative: bool,

    shift: u8,
    step: u8,

    // Live state (updated every tick)
    level: i16,
    counter: u32,
}

impl SweepEnvelope {
    pub fn new() -> Self {
        Self {
            enabled: false,
            mode_exponential: false,
            direction_decrease: false,
            phase_negative: false,
            shift: 0,
            step: 0,
            level: 0,
            counter: 0,
        }
    }

    pub fn read_level(&self) -> i16 {
        self.level
    }

    pub fn read_config(&self) -> u16 {
        let mut config: u16 = 0;
        if self.enabled {
            config |= 0x8000;
        }
        if self.mode_exponential {
            config |= 0x4000;
        }
        if self.direction_decrease {
            config |= 0x2000;
        }
        if self.phase_negative {
            config |= 0x1000;
        }
        config |= (self.shift as u16 & 0x1F) << 2;
        config |= self.step as u16 & 0x03;

        config
    }

    pub fn write_config(&mut self, config: u16) {
        self.enabled = (config & 0x8000) != 0;

        if !self.enabled {
            // Fixed mode: register holds volume / 2
            /*
            15    Must be zero      (0=Volume Mode)
            0-14  Voice volume/2    (-4000h..+3FFFh = Volume -8000h..+7FFEh)
            */
            self.level = ((config & 0x7FFF) as i16) << 1;
            return;
        } else {
            self.mode_exponential = (config & 0x4000) != 0;
            self.direction_decrease = (config & 0x2000) != 0;
            self.phase_negative = (config & 0x1000) != 0;
            self.shift = ((config >> 2) & 0x1F) as u8;
            self.step = (config & 0x03) as u8;
            self.counter = 0;
        }
    }
}

// Envelope generation and tick interface
impl SweepEnvelope {
    pub fn tick(&mut self) -> i16 {
        /*
        ; Precalculation, can be cached on phase begin.
        AdsrStep = 7 - StepValue
        IF Decreasing XOR PhaseNegative THEN
            AdsrStep = NOT AdsrStep ; +7,+6,+5,+4 => -8,-7,-6,-5
        AdsrStep = AdsrStep SHL Max(0,11-ShiftValue)
        CounterIncrement = 8000h SHR Max(0,ShiftValue-11)
        IF exponential AND increase AND AdsrLevel>6000h THEN
            IF ShiftValue < 10 THEN
            AdsrStep /= 4 ; SHR 2
            ELSE IF ShiftValue >= 11 THEN
            CounterIncrement /= 4 ; SHR 2
            ELSE
            AdsrStep /= 2 ; SHR 1
            CounterIncrement /= 2 ; SHR 1
        ELSE IF exponential AND decrease THEN
            AdsrStep=AdsrStep*AdsrLevel/8000h

        IF (StepValue | (ShiftValue SHL 2)) != ALL_BITS THEN
            CounterIncrement = MAX(CounterIncrement, 1)

        ; Runs once per 44.1kHz clock.
        Counter += CounterIncrement
        IF (Counter & 8000h) == 0 THEN
            RETURN ; No step this cycle.

        ; Saturate depending on mode.
        AdsrLevel = AdsrLevel + AdsrStep
        IF NOT decreasing THEN
            AdsrLevel = CLAMP(AdsrLevel, -8000h..+7FFFh)
        ELSE IF PhaseNegative THEN
            AdsrLevel = CLAMP(AdsrLevel, -8000h..0h)
        ELSE ; decreasing
            AdsrLevel = MAX(AdsrLevel, 0)
        */

        if !self.enabled {
            return self.level;
        }

        // Now just copy pseudocode
        let decreasing = self.direction_decrease;
        let phase_negative = self.phase_negative;
        let shift = self.shift;
        let step = self.step;
        let exponential = self.mode_exponential;

        let mut adsr_step: i32 = 7 - step as i32;
        if decreasing ^ phase_negative {
            adsr_step = !adsr_step;
        }

        let effective_shift = if !decreasing && exponential && self.level as i32 > 0x6000 {
            shift + 2
        } else {
            shift
        };

        adsr_step <<= 11u8.saturating_sub(effective_shift);

        if decreasing && exponential {
            adsr_step = (adsr_step * self.level as i32) >> 15;
        }

        let counter_shift = effective_shift.saturating_sub(11);
        let mut counter_increment = if counter_shift < 16 {
            0x8000u32 >> counter_shift
        } else {
            0
        };

        if counter_increment == 0 && (step != 3 || shift != 31) {
            counter_increment = 1;
        }

        self.counter = self.counter.wrapping_add(counter_increment);
        if (self.counter & 0x8000) == 0 {
            return self.level;
        }
        self.counter = 0;

        let new_level = self.level as i32 + adsr_step;

        self.level = if !decreasing {
            new_level.clamp(-0x8000, 0x7FFF) as i16
        } else if phase_negative {
            new_level.clamp(-0x8000, 0) as i16
        } else {
            new_level.max(0) as i16
        };

        self.level
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdsrPhase {
    Attack,
    Decay,
    Sustain,
    Release,
}
/// ADSR envelope generator per-voice, used to generate the volume of the channel during playback
#[derive(Debug, Clone, Copy)]
pub struct AdsrEnvelope {
    // Config (set by writing to the appropriate registers)
    attack_mode_exponential: bool,
    attack_shift: u8,
    attack_step: u8,
    decay_shift: u8,
    sustain_level: u16,
    sustain_mode_exponential: bool,
    sustain_direction_decrease: bool,
    sustain_shift: u8,
    sustain_step: u8,
    release_mode_exponential: bool,
    release_shift: u8,

    // Live state (updated every tick)
    phase: AdsrPhase,
    level: i16,
    counter: u32,
}

impl AdsrEnvelope {
    pub fn new() -> Self {
        Self {
            attack_mode_exponential: false,
            attack_shift: 0,
            attack_step: 0,
            decay_shift: 0,
            sustain_level: 0,
            sustain_mode_exponential: false,
            sustain_direction_decrease: false,
            sustain_shift: 0,
            sustain_step: 0,
            release_mode_exponential: false,
            release_shift: 0,
            phase: AdsrPhase::Release,
            level: 0,
            counter: 0,
        }
    }

    pub fn read_level(&self) -> i16 {
        self.level
    }

    pub fn phase(&self) -> AdsrPhase {
        self.phase
    }

    /*
    ____lower 16bit (at 1F801C08h+N*10h)___________________________________
    15    Attack Mode       (0=Linear, 1=Exponential)
    -     Attack Direction  (Fixed, always Increase) (until Level 7FFFh)
    14-10 Attack Shift      (0..1Fh = Fast..Slow)
    9-8   Attack Step       (0..3 = "+7,+6,+5,+4")
    -     Decay Mode        (Fixed, always Exponential)
    -     Decay Direction   (Fixed, always Decrease) (until Sustain Level)
    7-4   Decay Shift       (0..0Fh = Fast..Slow)
    -     Decay Step        (Fixed, always "-8")
    3-0   Sustain Level     (0..0Fh)  ;Level=(N+1)*800h
    ____upper 16bit (at 1F801C0Ah+N*10h)___________________________________
    31    Sustain Mode      (0=Linear, 1=Exponential)
    30    Sustain Direction (0=Increase, 1=Decrease) (until Key OFF flag)
    29    Not used?         (should be zero)
    28-24 Sustain Shift     (0..1Fh = Fast..Slow)
    23-22 Sustain Step      (0..3 = "+7,+6,+5,+4" or "-8,-7,-6,-5") (inc/dec)
    21    Release Mode      (0=Linear, 1=Exponential)
    -     Release Direction (Fixed, always Decrease) (until Level 0000h)
    20-16 Release Shift     (0..1Fh = Fast..Slow)
    -     Release Step      (Fixed, always "-8")
    */

    pub fn read_config_low(&self) -> u16 {
        let mut config_low: u16 = 0;
        if self.attack_mode_exponential {
            config_low |= 0x8000;
        }
        config_low |= (self.attack_shift as u16 & 0x1F) << 10;
        config_low |= (self.attack_step as u16 & 0x03) << 8;
        config_low |= (self.decay_shift as u16 & 0x0F) << 4;
        config_low |= ((self.sustain_level / 0x800 - 1) as u16) & 0x0F;

        config_low
    }

    pub fn read_config_high(&self) -> u16 {
        let mut config_high: u16 = 0;
        if self.sustain_mode_exponential {
            config_high |= 0x8000;
        }
        if self.sustain_direction_decrease {
            config_high |= 0x4000;
        }
        config_high |= (self.sustain_shift as u16 & 0x1F) << 8;
        config_high |= (self.sustain_step as u16 & 0x03) << 6;
        if self.release_mode_exponential {
            config_high |= 0x0020;
        }
        config_high |= self.release_shift as u16 & 0x1F;

        config_high
    }

    pub fn write_config_low(&mut self, config_low: u16) {
        self.attack_mode_exponential = (config_low & 0x8000) != 0;
        self.attack_shift = ((config_low >> 10) & 0x1F) as u8;
        self.attack_step = ((config_low >> 8) & 0x03) as u8;
        self.decay_shift = ((config_low >> 4) & 0x0F) as u8;
        self.sustain_level = ((config_low & 0x0F) as u16 + 1) << 11; // Shift by 11 as per psx-spx: "Level=(N+1)*800h"
    }

    pub fn write_config_high(&mut self, config_high: u16) {
        self.sustain_mode_exponential = (config_high & 0x8000) != 0;
        self.sustain_direction_decrease = (config_high & 0x4000) != 0;
        self.sustain_shift = ((config_high >> 8) & 0x1F) as u8;
        self.sustain_step = ((config_high >> 6) & 0x03) as u8;
        self.release_mode_exponential = (config_high & 0x0020) != 0;
        self.release_shift = (config_high & 0x1F) as u8;
    }

    pub fn key_on(&mut self) {
        self.phase = AdsrPhase::Attack;
        self.level = 0;
        self.counter = 0;
    }

    pub fn key_off(&mut self) {
        self.phase = AdsrPhase::Release;
    }

    pub fn force_end(&mut self) {
        self.level = 0;
        self.key_off();
    }
}

// Envelope generation and tick interface
impl AdsrEnvelope {
    /// Get (decreasing, shift, step, exponential) based on the current phase for use in precalculation
    /// of the envelope parameters
    fn current_params(&self) -> (bool, u8, u8, bool) {
        match self.phase {
            AdsrPhase::Attack => (
                false,
                self.attack_shift,
                self.attack_step,
                self.attack_mode_exponential,
            ),
            AdsrPhase::Decay => (true, self.decay_shift, 0, true),
            AdsrPhase::Sustain => (
                self.sustain_direction_decrease,
                self.sustain_shift,
                self.sustain_step,
                self.sustain_mode_exponential,
            ),
            AdsrPhase::Release => (true, self.release_shift, 0, self.release_mode_exponential),
        }
    }

    /// Called once every 768 cycles to update the envelope state machine and return the current volume
    pub fn tick(&mut self) -> i16 {
        /*
        AdsrStep = 7 - StepValue
        IF Decreasing XOR PhaseNegative THEN
            AdsrStep = NOT AdsrStep ; +7,+6,+5,+4 => -8,-7,-6,-5
        AdsrStep = AdsrStep SHL Max(0,11-ShiftValue)
        CounterIncrement = 8000h SHR Max(0,ShiftValue-11)
        IF exponential AND increase AND AdsrLevel>6000h THEN
            IF ShiftValue < 10 THEN
            AdsrStep /= 4 ; SHR 2
            ELSE IF ShiftValue >= 11 THEN
            CounterIncrement /= 4 ; SHR 2
            ELSE
            AdsrStep /= 2 ; SHR 1
            CounterIncrement /= 2 ; SHR 1
        ELSE IF exponential AND decrease THEN
            AdsrStep=AdsrStep*AdsrLevel/8000h

        IF (StepValue | (ShiftValue SHL 2)) != ALL_BITS THEN
            CounterIncrement = MAX(CounterIncrement, 1)

        ; Runs once per 44.1kHz clock.
        Counter += CounterIncrement
        IF (Counter & 8000h) == 0 THEN
            RETURN ; No step this cycle.

        ; Saturate depending on mode.
        AdsrLevel = AdsrLevel + AdsrStep
        IF NOT decreasing THEN
            AdsrLevel = CLAMP(AdsrLevel, -8000h..+7FFFh)
        ELSE IF PhaseNegative THEN
            AdsrLevel = CLAMP(AdsrLevel, -8000h..0h)
        ELSE ; decreasing
            AdsrLevel = MAX(AdsrLevel, 0)
        */

        // First check for automatic phase transitions
        if self.phase == AdsrPhase::Attack && self.level == 0x7FFF {
            self.phase = AdsrPhase::Decay;
        }
        if self.phase == AdsrPhase::Decay && self.level as i32 <= self.sustain_level as i32 {
            self.phase = AdsrPhase::Sustain;
        }

        let (decreasing, shift, step, exponential) = self.current_params();

        let mut adsr_step: i32 = 7 - step as i32;
        if decreasing {
            adsr_step = !adsr_step;
        }

        let effective_shift = if !decreasing && exponential && self.level > 0x6000 {
            shift + 2
        } else {
            shift
        };

        adsr_step <<= 11u8.saturating_sub(effective_shift);

        if decreasing && exponential {
            adsr_step = (adsr_step * self.level as i32) >> 15;
        }

        let counter_shift = effective_shift.saturating_sub(11);
        let mut counter_increment = if counter_shift < 16 {
            0x8000u32 >> counter_shift
        } else {
            0
        };

        if counter_increment == 0 && (step != 3 || shift != 31) {
            counter_increment = 1;
        }

        self.counter = self.counter.wrapping_add(counter_increment);
        if (self.counter & 0x8000) == 0 {
            return self.level;
        }
        self.counter = 0;

        let new_level_wide: i32 = self.level as i32 + adsr_step;

        self.level = if !decreasing {
            new_level_wide.clamp(-0x8000, 0x7FFF) as i16
        } else {
            new_level_wide.max(0).min(0x7FFF) as i16
        };

        self.level
    }
}

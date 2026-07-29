/*
 * @file /parastation-core/src/spu/mod.rs
 * @bri
 * PS1 Sound Processing Unit (SPU) implementation, including ADPCM decoding, envelope generation, voice management,
 * and mixing.
 *
 * Also provides the register interface for the system bus as well as DMA access to the SPU's internal RAM.
 * -----
 */

// Imports
mod adpcm;
mod envelope;
mod voice;
use voice::Voice;
mod mixer;
use mixer::Mixer;
mod backend;
pub use backend::SpuBackend;
mod interpolate;
mod reverb;
use reverb::Reverb;
mod noise;
use noise::NoiseGenerator;

use crate::elog;
use crate::interrupt_controller::{Interrupt, InterruptController};

// Shared SPU structures
/// 16 bit PCM sample, which is the output of the ADPCM decoder
#[derive(Debug, Clone, Copy)]
pub struct PcmSample(pub i16);

pub struct Spu {
    voices: [Voice; 24],
    mixer: Mixer,
    sound_ram: Box<[u8; 0x80000]>, // Heap allocated to avoid stack overflow

    // Data port/transfer state
    data_port_address: u32,
    data_transfer_control: u16,

    // IRQ
    irq_address: u16,
    irq_flag: bool,

    // Global control/status
    spu_control: u16, // SPUCNT
    spu_status: u16,  // SPUSTAT

    // Cached from previous writes, but should be write only technically
    kon_low_raw: u16,
    kon_high_raw: u16,
    koff_low_raw: u16,
    koff_high_raw: u16,

    // Reverb unit
    /*
    1F801D98h - Voice 0..23 Reverb mode aka Echo On (EON) (R/W)
    0-23  Voice 0..23 Destination (0=To Mixer, 1=To Mixer and to Reverb)
    */
    eon: [bool; 24],
    reverb: Reverb,

    pmon: [bool; 24], // Pitch modulation enable per voice
    non: [bool; 24],  // Use noise generator instead of ADPCM for this voice
    noise_generator: NoiseGenerator,

    // Backend
    backend: Box<dyn SpuBackend>,

    // Volume scaling
    cd_volume_left: i16,
    cd_volume_right: i16,
    extern_volume_left: i16,
    extern_volume_right: i16,
}

impl Spu {
    pub fn new(backend: Box<dyn SpuBackend>) -> Self {
        Self {
            voices: [Voice::new(); 24],
            mixer: Mixer::new(),
            sound_ram: Box::new([0; 0x80000]),
            data_port_address: 0,
            data_transfer_control: 0,
            irq_address: 0,
            irq_flag: false,
            spu_control: 0,
            spu_status: 0,
            kon_low_raw: 0,
            kon_high_raw: 0,
            koff_low_raw: 0,
            koff_high_raw: 0,
            eon: [false; 24],
            reverb: Reverb::new(),
            pmon: [false; 24],
            non: [false; 24],
            noise_generator: NoiseGenerator::new(),
            backend,
            cd_volume_left: 0x7FFF,
            cd_volume_right: 0x7FFF,
            extern_volume_left: 0x7FFF,
            extern_volume_right: 0x7FFF,
        }
    }

    // Register interface
    pub fn read_register(
        &mut self,
        offset: u32,
        interrupt_controller: &mut InterruptController,
    ) -> u16 {
        // until 0x1F801D80 (offset 0x180) are the voice registers, so see if we need to dispatch to a voice
        if offset < 0x180 {
            let voice_idx = (offset / 0x10) as usize;
            let voice_offset = offset % 0x10;
            return self.read_voice_register(voice_idx, voice_offset);
        }

        // Then the reverb register range is disjoint, so check for it here
        if matches!(offset, 0x184 | 0x186 | 0x1A2) || (0x1C0..=0x1FE).contains(&offset) {
            return self.reverb.read_register(offset as usize);
        }

        /*
        1F801D80h 4  Main Volume Left/Right
        1F801D84h 4  Reverb Output Volume Left/Right
        1F801D88h 4  Voice 0..23 Key ON (Start Attack/Decay/Sustain) (W)
        1F801D8Ch 4  Voice 0..23 Key OFF (Start Release) (W)
        1F801D90h 4  Voice 0..23 Channel FM (pitch lfo) mode (R/W)
        1F801D94h 4  Voice 0..23 Channel Noise mode (R/W)
        1F801D98h 4  Voice 0..23 Channel Reverb mode (R/W)
        1F801D9Ch 4  Voice 0..23 Channel ON/OFF (status) (R)
        1F801DA0h 2  Unknown? (R) or (W)
        1F801DA2h 2  Sound RAM Reverb Work Area Start Address
        1F801DA4h 2  Sound RAM IRQ Address
        1F801DA6h 2  Sound RAM Data Transfer Address
        1F801DA8h 2  Sound RAM Data Transfer Fifo
        1F801DAAh 2  SPU Control Register (SPUCNT)
        1F801DACh 2  Sound RAM Data Transfer Control
        1F801DAEh 2  SPU Status Register (SPUSTAT) (R)
        1F801DB0h 4  CD Volume Left/Right
        1F801DB4h 4  Extern Volume Left/Right
        1F801DB8h 4  Current Main Volume Left/Right
        1F801DBCh 4  Unknown? (R/W)
        */

        match offset {
            0x180 => self.mixer.read_main_volume_left() as u16,
            0x182 => self.mixer.read_main_volume_right() as u16,
            0x188 => self.kon_low_raw,
            0x18A => self.kon_high_raw,
            0x18C => self.koff_low_raw,
            0x18E => self.koff_high_raw,
            0x190 => self.read_pmon_low(),
            0x192 => self.read_pmon_high(),
            0x194 => self.read_non_low(),
            0x196 => self.read_non_high(),
            0x198 => self.read_eon_low(),
            0x19A => self.read_eon_high(),
            0x19C => self.read_channel_on_status_low(),
            0x19E => self.read_channel_on_status_high(),
            0x1A4 => self.irq_address,
            0x1A6 => (self.data_port_address >> 3) as u16, // Data transfer address is in 8 byte units
            0x1A8 => self.read_data_port(interrupt_controller),
            0x1AA => self.spu_control,
            0x1AC => self.data_transfer_control,
            0x1AE => self.spu_status,
            0x1B0 => self.cd_volume_left as u16,
            0x1B2 => self.cd_volume_right as u16,
            0x1B4 => self.extern_volume_left as u16,
            0x1B6 => self.extern_volume_right as u16,
            0x1B8 => self.mixer.read_main_volume_left() as u16,
            0x1BA => self.mixer.read_main_volume_right() as u16,
            _ => {
                elog!(
                    "Invalid/unhandled SPU read from register offset: 0x{:X}",
                    offset
                );
                0
            }
        }
    }

    pub fn write_register(
        &mut self,
        offset: u32,
        value: u16,
        interrupt_controller: &mut InterruptController,
    ) {
        // until 0x1F801D80 (offset 0x180) are the voice registers, so see if we need to dispatch to a voice
        if offset < 0x180 {
            let voice_idx = (offset / 0x10) as usize;
            let voice_offset = offset % 0x10;
            return self.write_voice_register(voice_idx, voice_offset, value);
        }

        // Then the reverb register range is disjoint, so check for it here
        if matches!(offset, 0x184 | 0x186 | 0x1A2) || (0x1C0..=0x1FE).contains(&offset) {
            return self.reverb.write_register(offset as usize, value);
        }

        /*
        1F801D80h 4  Main Volume Left/Right
        1F801D84h 4  Reverb Output Volume Left/Right
        1F801D88h 4  Voice 0..23 Key ON (Start Attack/Decay/Sustain) (W)
        1F801D8Ch 4  Voice 0..23 Key OFF (Start Release) (W)
        1F801D90h 4  Voice 0..23 Channel FM (pitch lfo) mode (R/W)
        1F801D94h 4  Voice 0..23 Channel Noise mode (R/W)
        1F801D98h 4  Voice 0..23 Channel Reverb mode (R/W)
        1F801D9Ch 4  Voice 0..23 Channel ON/OFF (status) (R)
        1F801DA0h 2  Unknown? (R) or (W)
        1F801DA2h 2  Sound RAM Reverb Work Area Start Address
        1F801DA4h 2  Sound RAM IRQ Address
        1F801DA6h 2  Sound RAM Data Transfer Address
        1F801DA8h 2  Sound RAM Data Transfer Fifo
        1F801DAAh 2  SPU Control Register (SPUCNT)
        1F801DACh 2  Sound RAM Data Transfer Control
        1F801DAEh 2  SPU Status Register (SPUSTAT) (R)
        1F801DB0h 4  CD Volume Left/Right
        1F801DB4h 4  Extern Volume Left/Right
        1F801DB8h 4  Current Main Volume Left/Right
        1F801DBCh 4  Unknown? (R/W)
        */

        match offset {
            0x180 => self.mixer.write_main_volume_left(value as i16),
            0x182 => self.mixer.write_main_volume_right(value as i16),
            0x188 => self.write_key_on_low(value),
            0x18A => self.write_key_on_high(value),
            0x18C => self.write_key_off_low(value),
            0x18E => self.write_key_off_high(value),
            0x190 => self.write_pmon_low(value),
            0x192 => self.write_pmon_high(value),
            0x194 => self.write_non_low(value),
            0x196 => self.write_non_high(value),
            0x198 => self.write_eon_low(value),
            0x19A => self.write_eon_high(value),
            0x1A4 => self.irq_address = value,
            0x1A6 => self.data_port_address = (value as u32) << 3, // Data transfer address is in 8 byte units
            0x1A8 => self.write_data_port(value, interrupt_controller),
            0x1AA => self.write_spu_control(value),
            0x1AC => self.data_transfer_control = value,
            0x1B0 => self.cd_volume_left = value as i16,
            0x1B2 => self.cd_volume_right = value as i16,
            0x1B4 => self.extern_volume_left = value as i16,
            0x1B6 => self.extern_volume_right = value as i16,
            0x1B8 => self.mixer.write_main_volume_left(value as i16),
            0x1BA => self.mixer.write_main_volume_right(value as i16),
            _ => {
                elog!(
                    "Invalid/unhandled SPU write to register offset: 0x{:X}",
                    offset
                );
            }
        }
    }

    // Per voice register dispatch
    /*
    1F801C00h+N*10h 4   Voice 0..23 Volume Left/Right
    1F801C04h+N*10h 2   Voice 0..23 ADPCM Sample Rate
    1F801C06h+N*10h 2   Voice 0..23 ADPCM Start Address
    1F801C08h+N*10h 4   Voice 0..23 ADSR Attack/Decay/Sustain/Release
    1F801C0Ch+N*10h 2   Voice 0..23 ADSR Current Volume
    1F801C0Eh+N*10h 2   Voice 0..23 ADPCM Repeat Address
    */
    fn read_voice_register(&self, voice_idx: usize, offset: u32) -> u16 {
        match offset {
            0x0 => self.voices[voice_idx].read_sweep_config_left(),
            0x2 => self.voices[voice_idx].read_sweep_config_right(),
            0x4 => self.voices[voice_idx].read_sample_rate(),
            0x6 => self.voices[voice_idx].read_start_address(),
            0x8 => self.voices[voice_idx].read_adsr_config_low(),
            0xA => self.voices[voice_idx].read_adsr_config_high(),
            0xC => self.voices[voice_idx].read_current_level() as u16,
            0xE => self.voices[voice_idx].read_repeat_address(),
            _ => {
                // elog!("Invalid voice register offset:  for read0x{:X}", offset);
                0
            }
        }
    }

    fn write_voice_register(&mut self, voice_idx: usize, offset: u32, value: u16) {
        match offset {
            0x0 => self.voices[voice_idx].write_sweep_config_left(value),
            0x2 => self.voices[voice_idx].write_sweep_config_right(value),
            0x4 => self.voices[voice_idx].write_sample_rate(value),
            0x6 => self.voices[voice_idx].write_start_address(value),
            0x8 => self.voices[voice_idx].write_adsr_config_low(value),
            0xA => self.voices[voice_idx].write_adsr_config_high(value),
            0xE => self.voices[voice_idx].write_repeat_address(value),
            _ => {} // elog!("Invalid voice register offset:  for write0x{:X}", offset),
        }
    }

    pub fn read_data_port(&mut self, interrupt_controller: &mut InterruptController) -> u16 {
        let addr = self.data_port_address as usize;
        let value = (self.sound_ram[addr] as u16) | ((self.sound_ram[addr + 1] as u16) << 8);

        self.check_spu_ram_irq(addr as u32, interrupt_controller);

        self.data_port_address = self.data_port_address.wrapping_add(2);
        self.data_port_address &= 0x7FFFF;

        value
    }

    pub fn write_data_port(&mut self, value: u16, interrupt_controller: &mut InterruptController) {
        let addr = self.data_port_address as usize;
        self.sound_ram[addr] = (value & 0xFF) as u8;
        self.sound_ram[addr + 1] = (value >> 8) as u8;

        self.check_spu_ram_irq(addr as u32, interrupt_controller);

        self.data_port_address = self.data_port_address.wrapping_add(2);
        self.data_port_address &= 0x7FFFF;
    }

    fn write_spu_control(&mut self, value: u16) {
        // First update the control register
        self.spu_control = value;

        /*
        15    SPU Enable              (0=Off, 1=On)       (Don't care for CD Audio)
        14    Mute SPU                (0=Mute, 1=Unmute)  (Don't care for CD Audio)
        13-10 Noise Frequency Shift   (0..0Fh = Low .. High Frequency)
        9-8   Noise Frequency Step    (0..03h = Step "4,5,6,7")
        7     Reverb Master Enable    (0=Disabled, 1=Enabled)
        6     IRQ9 Enable (0=Disabled/Acknowledge, 1=Enabled; only when Bit15=1)
        5-4   Sound RAM Transfer Mode (0=Stop, 1=ManualWrite, 2=DMAwrite, 3=DMAread)
        3     External Audio Reverb   (0=Off, 1=On)
        2     CD Audio Reverb         (0=Off, 1=On) (for CD-DA and XA-ADPCM)
        1     External Audio Enable   (0=Off, 1=On)
        0     CD Audio Enable         (0=Off, 1=On) (for CD-DA and XA-ADPCM)
        */

        // Reverb enable
        self.reverb.set_enabled(value & 0x80 != 0);

        // IRQ9 enable and acknowledge
        if value & 0x40 == 0 {
            self.spu_status &= !0x0040;
        }
    }

    fn read_channel_on_status_low(&self) -> u16 {
        let mut status: u16 = 0;
        for i in 0..16 {
            if self.voices[i].endx() {
                status |= 1 << i;
            }
        }
        status
    }

    fn read_channel_on_status_high(&self) -> u16 {
        let mut status: u16 = 0;
        for i in 16..24 {
            if self.voices[i].endx() {
                status |= 1 << (i - 16);
            }
        }
        status
    }

    fn write_key_on_low(&mut self, value: u16) {
        self.kon_low_raw = value;
        for i in 0..16 {
            if (value & (1 << i)) != 0 {
                self.voices[i].key_on(self.sound_ram.as_slice());
            }
        }
    }

    fn write_key_on_high(&mut self, value: u16) {
        self.kon_high_raw = value;
        for i in 16..24 {
            if (value & (1 << (i - 16))) != 0 {
                self.voices[i].key_on(self.sound_ram.as_slice());
            }
        }
    }

    fn write_key_off_low(&mut self, value: u16) {
        self.koff_low_raw = value;
        for i in 0..16 {
            if (value & (1 << i)) != 0 {
                self.voices[i].key_off();
            }
        }
    }

    fn write_key_off_high(&mut self, value: u16) {
        self.koff_high_raw = value;
        for i in 16..24 {
            if (value & (1 << (i - 16))) != 0 {
                self.voices[i].key_off();
            }
        }
    }

    fn read_eon_low(&self) -> u16 {
        let mut eon: u16 = 0;
        for i in 0..16 {
            if self.eon[i] {
                eon |= 1 << i;
            }
        }
        eon
    }

    fn read_eon_high(&self) -> u16 {
        let mut eon: u16 = 0;
        for i in 16..24 {
            if self.eon[i] {
                eon |= 1 << (i - 16);
            }
        }
        eon
    }

    fn write_eon_low(&mut self, value: u16) {
        for i in 0..16 {
            self.eon[i] = (value & (1 << i)) != 0;
        }
    }

    fn write_eon_high(&mut self, value: u16) {
        for i in 16..24 {
            self.eon[i] = (value & (1 << (i - 16))) != 0;
        }
    }

    fn read_pmon_low(&self) -> u16 {
        let mut pmon: u16 = 0;
        for i in 0..16 {
            if self.pmon[i] {
                pmon |= 1 << i;
            }
        }
        pmon
    }

    fn read_pmon_high(&self) -> u16 {
        let mut pmon: u16 = 0;
        for i in 16..24 {
            if self.pmon[i] {
                pmon |= 1 << (i - 16);
            }
        }
        pmon
    }

    fn write_pmon_low(&mut self, value: u16) {
        for i in 0..16 {
            self.pmon[i] = (value & (1 << i)) != 0;
        }
    }

    fn write_pmon_high(&mut self, value: u16) {
        for i in 16..24 {
            self.pmon[i] = (value & (1 << (i - 16))) != 0;
        }
    }

    fn read_non_low(&self) -> u16 {
        let mut non: u16 = 0;
        for i in 0..16 {
            if self.non[i] {
                non |= 1 << i;
            }
        }
        non
    }

    fn read_non_high(&self) -> u16 {
        let mut non: u16 = 0;
        for i in 16..24 {
            if self.non[i] {
                non |= 1 << (i - 16);
            }
        }
        non
    }

    fn write_non_low(&mut self, value: u16) {
        for i in 0..16 {
            self.non[i] = (value & (1 << i)) != 0;
        }
    }

    fn write_non_high(&mut self, value: u16) {
        for i in 16..24 {
            self.non[i] = (value & (1 << (i - 16))) != 0;
        }
    }
}

// Interrupt handling
impl Spu {
    /// Byte address of the SPU RAM address pointed to by the IRQ address register
    fn irq_byte_address(&self) -> u32 {
        (self.irq_address as u32) * 8
    }

    fn irq9_enabled(&self) -> bool {
        self.spu_control & 0x8000 != 0 && self.spu_control & 0x0040 != 0
    }

    /// Raises IRQ9 if the IRQ address is reached and IRQ9 is enabled
    fn check_spu_ram_irq(
        &mut self,
        accessed_byte_addr: u32,
        interrupt_controller: &mut InterruptController,
    ) {
        if !self.irq9_enabled() {
            return;
        }
        if accessed_byte_addr != self.irq_byte_address() {
            return;
        }

        // Set SPUSTAT bit 6 (IRQ9 flag) and raise the interrupt.
        self.spu_status |= 0x0040;
        interrupt_controller.raise_interrupt(Interrupt::SPU);
    }
}

// SPU ticking to advance the state of the voices and mixer
impl Spu {
    /// Tick the SPU for one sample period, advancing the state of all voices and returning the mixed output sample
    pub fn tick(
        &mut self,
        cd_sample: (PcmSample, PcmSample),
        interrupt_controller: &mut InterruptController,
    ) -> (PcmSample, PcmSample) {
        let mut samples = [PcmSample(0); 24];
        let mut envelope_volumes = [0i16; 24];
        let mut voice_volume_left = [0i16; 24];
        let mut voice_volume_right = [0i16; 24];

        self.noise_generator.tick();

        let mut prev_amplitude: i16 = 0;
        for i in 0..24 {
            let pmon_enabled = self.pmon[i] && i != 0;
            let non_enabled = self.non[i];
            let (sample, fetched_addr) = self.voices[i].tick(
                &self.sound_ram[..],
                pmon_enabled,
                prev_amplitude,
                non_enabled,
                self.noise_generator.output(),
            );
            samples[i] = sample;

            if let Some(addr) = fetched_addr {
                self.check_spu_ram_irq(addr, interrupt_controller);
            }

            envelope_volumes[i] = self.voices[i].current_envelope_volume();
            voice_volume_left[i] = self.voices[i].current_sweep_volume_left();
            voice_volume_right[i] = self.voices[i].current_sweep_volume_right();

            prev_amplitude = self.voices[i].current_amplitude();
        }

        let (dry_left, dry_right) = self.mixer.mix(
            &samples,
            &envelope_volumes,
            &voice_volume_left,
            &voice_volume_right,
        );

        // Apply main volume to DRY ONLY, before combining with reverb
        let dry_left_scaled = ((dry_left.0 as i32 * self.mixer.read_main_volume_left() as i32)
            >> 15)
            .clamp(-0x8000, 0x7FFF) as i16;
        let dry_right_scaled = ((dry_right.0 as i32 * self.mixer.read_main_volume_right() as i32)
            >> 15)
            .clamp(-0x8000, 0x7FFF) as i16;

        let mut reverb_input_l: i32 = 0;
        let mut reverb_input_r: i32 = 0;
        for i in 0..24 {
            if !self.eon[i] {
                continue;
            }
            let post_envelope = (samples[i].0 as i32 * envelope_volumes[i] as i32) >> 15;
            self.voices[i].set_current_amplitude(post_envelope);

            reverb_input_l += (post_envelope * voice_volume_left[i] as i32) >> 15;
            reverb_input_r += (post_envelope * voice_volume_right[i] as i32) >> 15;
        }
        let reverb_input = (
            reverb_input_l.clamp(-0x8000, 0x7FFF) as i16,
            reverb_input_r.clamp(-0x8000, 0x7FFF) as i16,
        );

        self.reverb.tick(reverb_input, &mut self.sound_ram[..]);
        let (reverb_left, reverb_right) = self.reverb.current_output();

        // Combine main-volume-scaled dry with FULLY-SCALED reverb output (no further scaling)
        let final_left =
            (dry_left_scaled as i32 + reverb_left.0 as i32).clamp(-0x8000, 0x7FFF) as i16;
        let final_right =
            (dry_right_scaled as i32 + reverb_right.0 as i32).clamp(-0x8000, 0x7FFF) as i16;

        let (cd_l, cd_r) = cd_sample;

        // Scale CD audio by CD volume before mixing in
        let cd_left_scaled =
            ((cd_l.0 as i32 * self.cd_volume_left as i32) >> 15).clamp(-0x8000, 0x7FFF) as i16;
        let cd_right_scaled =
            ((cd_r.0 as i32 * self.cd_volume_right as i32) >> 15).clamp(-0x8000, 0x7FFF) as i16;

        // Mix in CD audio last, after voices + reverb
        let mixed_left = (final_left as i32 + cd_left_scaled as i32).clamp(-0x8000, 0x7FFF) as i16;
        let mixed_right =
            (final_right as i32 + cd_right_scaled as i32).clamp(-0x8000, 0x7FFF) as i16;

        (PcmSample(mixed_left), PcmSample(mixed_right))
    }

    pub fn handle_event(
        &mut self,
        cd_sample: (PcmSample, PcmSample),
        interrupt_controller: &mut InterruptController,
    ) {
        let (left, right) = self.tick(cd_sample, interrupt_controller);

        if self.backend.buffer_available() > 0 {
            self.backend.push_sample(left, right);
        }
    }
}

/*
 * @file /parastation-core/src/spu/reverb.rs
 * @bri
 * Reverb unit for the PS1's SPU, which is a 39-tap FIR filter applying reverb effects to the SPU's output.
 * Has quite an extensive register file and list of filters, so most functions have links to the relevant psx-spx
 * and jsgroth.dev blog posts for reference.
 *
 * https://jsgroth.dev/blog/posts/ps1-spu-part-3/
 * https://psx-spx.consoledev.net/soundprocessingunitspu/#spu-reverb-registers
 * https://psx-spx.consoledev.net/soundprocessingunitspu/#spu-reverb-formula
 * -----
 */

// Imports
use crate::spu::PcmSample;

// 39-tap FIR filter coefficiens from psx-spx, used for the reverb unit
const FIR_COEFFS: [i32; 39] = [
    -0x0001, 0x0000, 0x0002, 0x0000, -0x000A, 0x0000, 0x0023, 0x0000, -0x0067, 0x0000, 0x010A,
    0x0000, -0x0268, 0x0000, 0x0534, 0x0000, -0x0B90, 0x0000, 0x2806, 0x4000, 0x2806, 0x0000,
    -0x0B90, 0x0000, 0x0534, 0x0000, -0x0268, 0x0000, 0x010A, 0x0000, -0x0067, 0x0000, 0x0023,
    0x0000, -0x000A, 0x0000, 0x0002, 0x0000, -0x0001,
];

/// A 39-sample ring buffer used for the FIR filter in the reverb unit
struct FirRingBuffer {
    samples: [i32; 39],
    pos: usize,
}

impl FirRingBuffer {
    fn new() -> Self {
        Self {
            samples: [0; 39],
            pos: 0,
        }
    }

    fn push(&mut self, value: i32) {
        self.samples[self.pos] = value;
        self.pos = (self.pos + 1) % 39;
    }

    fn filter(&self) -> i32 {
        let mut sum: i64 = 0;
        for i in 0..39 {
            // samples[pos] is the oldest sample right after a push (about to be overwritten next), so read starting
            // from pos to get the newest sample first
            let idx = (self.pos + i) % 39;
            sum += FIR_COEFFS[i] as i64 * self.samples[idx] as i64;
        }
        (sum >> 15) as i32
    }
}

pub struct Reverb {
    // Config (set by register writes)
    pub vlout: i16,
    pub vrout: i16,
    pub mbase: u16,
    pub dapf1: u16,
    pub dapf2: u16,
    pub viir: i16,
    pub vcomb1: i16,
    pub vcomb2: i16,
    pub vcomb3: i16,
    pub vcomb4: i16,
    pub vwall: i16,
    pub vapf1: i16,
    pub vapf2: i16,
    pub mlsame: u16,
    pub mrsame: u16,
    pub mlcomb1: u16,
    pub mrcomb1: u16,
    pub mlcomb2: u16,
    pub mrcomb2: u16,
    pub dlsame: u16,
    pub drsame: u16,
    pub mldiff: u16,
    pub mrdiff: u16,
    pub mlcomb3: u16,
    pub mrcomb3: u16,
    pub mlcomb4: u16,
    pub mrcomb4: u16,
    pub dldiff: u16,
    pub drdiff: u16,
    pub mlapf1: u16,
    pub mrapf1: u16,
    pub mlapf2: u16,
    pub mrapf2: u16,
    pub vlin: i16,
    pub vrin: i16,

    // Live state
    pub current_output: (PcmSample, PcmSample),
    enabled: bool,
    current_buffer_offset: u32,
    odd_tick: bool, // Process left on even ticks, right on odd ticks

    // FIR filter ring buffers for the left and right channels
    input_buffer_l: FirRingBuffer,
    input_buffer_r: FirRingBuffer,
    output_buffer_l: FirRingBuffer,
    output_buffer_r: FirRingBuffer,
}

impl Reverb {
    pub fn new() -> Self {
        Self {
            vlout: 0,
            vrout: 0,
            mbase: 0,
            dapf1: 0,
            dapf2: 0,
            viir: 0,
            vcomb1: 0,
            vcomb2: 0,
            vcomb3: 0,
            vcomb4: 0,
            vwall: 0,
            vapf1: 0,
            vapf2: 0,
            mlsame: 0,
            mrsame: 0,
            mlcomb1: 0,
            mrcomb1: 0,
            mlcomb2: 0,
            mrcomb2: 0,
            dlsame: 0,
            drsame: 0,
            mldiff: 0,
            mrdiff: 0,
            mlcomb3: 0,
            mrcomb3: 0,
            mlcomb4: 0,
            mrcomb4: 0,
            dldiff: 0,
            drdiff: 0,
            mlapf1: 0,
            mrapf1: 0,
            mlapf2: 0,
            mrapf2: 0,
            vlin: 0,
            vrin: 0,
            current_output: (PcmSample(0), PcmSample(0)),
            enabled: false,
            current_buffer_offset: 0,
            odd_tick: false,
            input_buffer_l: FirRingBuffer::new(),
            input_buffer_r: FirRingBuffer::new(),
            output_buffer_l: FirRingBuffer::new(),
            output_buffer_r: FirRingBuffer::new(),
        }
    }

    // Register interface
    /*
    Port      Reg   Name    Type    Expl.
    1F801D84h spu   vLOUT   volume  Reverb Output Volume Left
    1F801D86h spu   vROUT   volume  Reverb Output Volume Right
    1F801DA2h spu   mBASE   base    Reverb Work Area Start Address in Sound RAM
    1F801DC0h rev00 dAPF1   disp    Reverb APF Offset 1
    1F801DC2h rev01 dAPF2   disp    Reverb APF Offset 2
    1F801DC4h rev02 vIIR    volume  Reverb Reflection Volume 1
    1F801DC6h rev03 vCOMB1  volume  Reverb Comb Volume 1
    1F801DC8h rev04 vCOMB2  volume  Reverb Comb Volume 2
    1F801DCAh rev05 vCOMB3  volume  Reverb Comb Volume 3
    1F801DCCh rev06 vCOMB4  volume  Reverb Comb Volume 4
    1F801DCEh rev07 vWALL   volume  Reverb Reflection Volume 2
    1F801DD0h rev08 vAPF1   volume  Reverb APF Volume 1
    1F801DD2h rev09 vAPF2   volume  Reverb APF Volume 2
    1F801DD4h rev0A mLSAME  src/dst Reverb Same Side Reflection Address 1 Left
    1F801DD6h rev0B mRSAME  src/dst Reverb Same Side Reflection Address 1 Right
    1F801DD8h rev0C mLCOMB1 src     Reverb Comb Address 1 Left
    1F801DDAh rev0D mRCOMB1 src     Reverb Comb Address 1 Right
    1F801DDCh rev0E mLCOMB2 src     Reverb Comb Address 2 Left
    1F801DDEh rev0F mRCOMB2 src     Reverb Comb Address 2 Right
    1F801DE0h rev10 dLSAME  src     Reverb Same Side Reflection Address 2 Left
    1F801DE2h rev11 dRSAME  src     Reverb Same Side Reflection Address 2 Right
    1F801DE4h rev12 mLDIFF  src/dst Reverb Different Side Reflect Address 1 Left
    1F801DE6h rev13 mRDIFF  src/dst Reverb Different Side Reflect Address 1 Right
    1F801DE8h rev14 mLCOMB3 src     Reverb Comb Address 3 Left
    1F801DEAh rev15 mRCOMB3 src     Reverb Comb Address 3 Right
    1F801DECh rev16 mLCOMB4 src     Reverb Comb Address 4 Left
    1F801DEEh rev17 mRCOMB4 src     Reverb Comb Address 4 Right
    1F801DF0h rev18 dLDIFF  src     Reverb Different Side Reflect Address 2 Left
    1F801DF2h rev19 dRDIFF  src     Reverb Different Side Reflect Address 2 Right
    1F801DF4h rev1A mLAPF1  src/dst Reverb APF Address 1 Left
    1F801DF6h rev1B mRAPF1  src/dst Reverb APF Address 1 Right
    1F801DF8h rev1C mLAPF2  src/dst Reverb APF Address 2 Left
    1F801DFAh rev1D mRAPF2  src/dst Reverb APF Address 2 Right
    1F801DFCh rev1E vLIN    volume  Reverb Input Volume Left
    1F801DFEh rev1F vRIN    volume  Reverb Input Volume Right
    */
    pub fn read_register(&self, offset: usize) -> u16 {
        // Start from the SPU base address 0x1F801C00
        match offset {
            0x184 => self.vlout as u16,
            0x186 => self.vrout as u16,
            0x1A2 => self.mbase,
            0x1C0 => self.dapf1,
            0x1C2 => self.dapf2,
            0x1C4 => self.viir as u16,
            0x1C6 => self.vcomb1 as u16,
            0x1C8 => self.vcomb2 as u16,
            0x1CA => self.vcomb3 as u16,
            0x1CC => self.vcomb4 as u16,
            0x1CE => self.vwall as u16,
            0x1D0 => self.vapf1 as u16,
            0x1D2 => self.vapf2 as u16,
            0x1D4 => self.mlsame,
            0x1D6 => self.mrsame,
            0x1D8 => self.mlcomb1,
            0x1DA => self.mrcomb1,
            0x1DC => self.mlcomb2,
            0x1DE => self.mrcomb2,
            0x1E0 => self.dlsame,
            0x1E2 => self.drsame,
            0x1E4 => self.mldiff,
            0x1E6 => self.mrdiff,
            0x1E8 => self.mlcomb3,
            0x1EA => self.mrcomb3,
            0x1EC => self.mlcomb4,
            0x1EE => self.mrcomb4,
            0x1F0 => self.dldiff,
            0x1F2 => self.drdiff,
            0x1F4 => self.mlapf1,
            0x1F6 => self.mrapf1,
            0x1F8 => self.mlapf2,
            0x1FA => self.mrapf2,
            0x1FC => self.vlin as u16,
            0x1FE => self.vrin as u16,
            _ => {
                eprintln!("Reverb: Invalid register read at offset {offset:#X}");
                0
            }
        }
    }

    pub fn write_register(&mut self, offset: usize, value: u16) {
        match offset {
            0x184 => self.vlout = value as i16,
            0x186 => self.vrout = value as i16,
            0x1A2 => self.write_mbase(value),
            0x1C0 => self.dapf1 = value,
            0x1C2 => self.dapf2 = value,
            0x1C4 => self.viir = value as i16,
            0x1C6 => self.vcomb1 = value as i16,
            0x1C8 => self.vcomb2 = value as i16,
            0x1CA => self.vcomb3 = value as i16,
            0x1CC => self.vcomb4 = value as i16,
            0x1CE => self.vwall = value as i16,
            0x1D0 => self.vapf1 = value as i16,
            0x1D2 => self.vapf2 = value as i16,
            0x1D4 => self.mlsame = value,
            0x1D6 => self.mrsame = value,
            0x1D8 => self.mlcomb1 = value,
            0x1DA => self.mrcomb1 = value,
            0x1DC => self.mlcomb2 = value,
            0x1DE => self.mrcomb2 = value,
            0x1E0 => self.dlsame = value,
            0x1E2 => self.drsame = value,
            0x1E4 => self.mldiff = value,
            0x1E6 => self.mrdiff = value,
            0x1E8 => self.mlcomb3 = value,
            0x1EA => self.mrcomb3 = value,
            0x1EC => self.mlcomb4 = value,
            0x1EE => self.mrcomb4 = value,
            0x1F0 => self.dldiff = value,
            0x1F2 => self.drdiff = value,
            0x1F4 => self.mlapf1 = value,
            0x1F6 => self.mrapf1 = value,
            0x1F8 => self.mlapf2 = value,
            0x1FA => self.mrapf2 = value,
            0x1FC => self.vlin = value as i16,
            0x1FE => self.vrin = value as i16,
            _ => {
                eprintln!("Reverb: Invalid register write at offset {offset:#X}");
            }
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn current_output(&self) -> (PcmSample, PcmSample) {
        self.current_output
    }
}

// Reverb passes
impl Reverb {
    pub fn write_mbase(&mut self, value: u16) {
        self.mbase = value;
        self.current_buffer_offset = 0;
    }

    fn work_area_size(&self) -> usize {
        let base = (self.mbase as usize) << 3;
        0x7FFFE_usize.saturating_sub(base).max(1)
    }

    fn resolve_address(&self, register_offset: u16, byte_adjustment: i32) -> usize {
        // Resolves a work-area relative offset register into an absolute address in the SPU's RAM
        /*
        How does the SPU fit all of these FIFO buffers into a single circular buffer? The answer is actually pretty
        simple - all FIFO buffer addresses are relative to the current circular buffer head rather than being absolute
        addresses, and all FIFO buffers wrap within the circular buffer. Advancing the circular buffer head by 1 sample
        automatically moves every FIFO buffer forward by 1 sample, effectively popping off the last sample in each FIFO
        buffer.
        */

        // register_offset is a raw src/dst/disp register value in 8 byte units. byte_adjustment is applied after
        // scaling for byte level adjustments that are expressed in the pseudocode

        let base = (self.mbase as usize) << 3;
        let scaled = (register_offset as i32) << 3;
        let absolute_offset = (scaled + byte_adjustment) as i64 + self.current_buffer_offset as i64;
        let wrapped = absolute_offset.rem_euclid(self.work_area_size() as i64) as usize;
        base + wrapped
    }

    fn mem_read(&self, sound_ram: &[u8], register_offset: u16, byte_adjustment: i32) -> i16 {
        let addr = self.resolve_address(register_offset, byte_adjustment);
        i16::from_le_bytes([sound_ram[addr], sound_ram[addr + 1]])
    }

    fn mem_write(
        &self,
        sound_ram: &mut [u8],
        register_offset: u16,
        byte_adjustment: i32,
        value: i32,
    ) {
        /*
        The SPUCNT register contains a Reverb Master Enable flag, and Reverb Enable flags for External Audio input and
        CD Audio input.
        When the Reverb Master Enable flag is cleared, the SPU stops to write any data to the Reverb buffer (that is
        useful when zero-filling the reverb buffer; ensuring that already-zero values aren't overwritten by
        still-nonzero values).
        However, the Reverb Master Enable flag does not disable output from Reverb buffer to the speakers
        (that might be useful to output uncompressed 22050Hz samples) (otherwise, to disable the buffer output, set the
        Reverb Output volume to zero and/or zerofill the reverb buffer).
        */
        // Beacuse of above, we don't perform any mutations to the reverb buffer if the reverb unit is disabled
        if !self.enabled {
            return;
        }

        let addr = self.resolve_address(register_offset, byte_adjustment);
        let clamped = value.clamp(-0x8000, 0x7FFF) as i16;
        let bytes = clamped.to_le_bytes();
        sound_ram[addr] = bytes[0];
        sound_ram[addr + 1] = bytes[1];
    }

    // Fixed point multiply helper, same one used in the mixer
    fn mul(a: i32, coef: i16) -> i32 {
        (a * coef as i32) >> 15
    }

    // Passes

    // Reflection stage pass, used by same-side and different-side reflection passes
    fn reflection_stage(
        &self,
        sound_ram: &mut [u8],
        input: i32,
        dest_offset: u16,
        tap_offset: u16,
    ) -> i32 {
        /*
        ____Same Side Reflection (left-to-left and right-to-right)___________________
        [mLSAME] = (Lin + [dLSAME]*vWALL - [mLSAME-2])*vIIR + [mLSAME-2]  ;L-to-L
        [mRSAME] = (Rin + [dRSAME]*vWALL - [mRSAME-2])*vIIR + [mRSAME-2]  ;R-to-R
        ___Different Side Reflection (left-to-right and right-to-left)_______________
        [mLDIFF] = (Lin + [dRDIFF]*vWALL - [mLDIFF-2])*vIIR + [mLDIFF-2]  ;R-to-L
        [mRDIFF] = (Rin + [dLDIFF]*vWALL - [mRDIFF-2])*vIIR + [mRDIFF-2]  ;L-to-R
        */

        let prev = self.mem_read(sound_ram, dest_offset, -2) as i32;
        let tap = self.mem_read(sound_ram, tap_offset, 0) as i32;

        let inner = (input + Self::mul(tap, self.vwall) - prev).clamp(-0x8000, 0x7FFF);
        let result = prev + Self::mul(inner, self.viir);

        self.mem_write(sound_ram, dest_offset, 0, result);
        result
    }

    // Sum of 4 samples multiplied by their respective coefficients, used in the comb passes
    fn comb_sum(&self, sound_ram: &[u8], offsets: [u16; 4], coefs: [i16; 4]) -> i32 {
        (0..4)
            .map(|i| Self::mul(self.mem_read(sound_ram, offsets[i], 0) as i32, coefs[i]))
            .sum::<i32>()
            .clamp(-0x8000, 0x7FFF)
    }

    // All pass stage, used by the APF passes
    fn allpass_stage(
        &self,
        sound_ram: &mut [u8],
        out: i32,
        dest_offset: u16,
        disp: u16,
        vapf: i16,
    ) -> i32 {
        /*
        ___Late Reverb APF1 (All Pass Filter 1, with input from COMB)________________
        Lout=Lout-vAPF1*[mLAPF1-dAPF1], [mLAPF1]=Lout, Lout=Lout*vAPF1+[mLAPF1-dAPF1]
        Rout=Rout-vAPF1*[mRAPF1-dAPF1], [mRAPF1]=Rout, Rout=Rout*vAPF1+[mRAPF1-dAPF1]
        ___Late Reverb APF2 (All Pass Filter 2, with input from APF1)________________
        Lout=Lout-vAPF2*[mLAPF2-dAPF2], [mLAPF2]=Lout, Lout=Lout*vAPF2+[mLAPF2-dAPF2]
        Rout=Rout-vAPF2*[mRAPF2-dAPF2], [mRAPF2]=Rout, Rout=Rout*vAPF2+[mRAPF2-dAPF2]
        */

        let disp_bytes = (disp as i32) << 3;
        let tap = self.mem_read(sound_ram, dest_offset, -disp_bytes) as i32;

        let new_apf_sample = (out - Self::mul(tap, vapf)).clamp(-0x8000, 0x7FFF);
        self.mem_write(sound_ram, dest_offset, 0, new_apf_sample);

        tap + Self::mul(new_apf_sample, vapf)
    }
}

// Public reverb processing interface
impl Reverb {
    fn channel_fields(&self) -> (i16, u16, u16, u16, u16, [u16; 4], u16, u16) {
        // Return (vlin_or_vrin, dsame_tap, msame_dest, ddiff_tap, mdiff_dest,comb_addrs, apf1_addr, apf2_addr)

        if !self.odd_tick {
            // Left channel
            (
                self.vlin,
                self.dlsame,
                self.mlsame,
                self.drdiff, // different-side reflection tap for LEFT reads the RIGHT diff register
                self.mldiff,
                [self.mlcomb1, self.mlcomb2, self.mlcomb3, self.mlcomb4],
                self.mlapf1,
                self.mlapf2,
            )
        } else {
            // Right channel
            (
                self.vrin,
                self.drsame,
                self.mrsame,
                self.dldiff, // different-side reflection tap for RIGHT reads the LEFT diff register
                self.mrdiff,
                [self.mrcomb1, self.mrcomb2, self.mrcomb3, self.mrcomb4],
                self.mrapf1,
                self.mrapf2,
            )
        }
    }

    pub fn tick(&mut self, dry_input: (i16, i16), sound_ram: &mut [u8]) {
        /*
        ___Input from Mixer (Input volume multiplied with incoming data)_____________
        Lin = vLIN * LeftInput    ;from any channels that have Reverb enabled
        Rin = vRIN * RightInput   ;from any channels that have Reverb enabled
        ____Same Side Reflection (left-to-left and right-to-right)___________________
        [mLSAME] = (Lin + [dLSAME]*vWALL - [mLSAME-2])*vIIR + [mLSAME-2]  ;L-to-L
        [mRSAME] = (Rin + [dRSAME]*vWALL - [mRSAME-2])*vIIR + [mRSAME-2]  ;R-to-R
        ___Different Side Reflection (left-to-right and right-to-left)_______________
        [mLDIFF] = (Lin + [dRDIFF]*vWALL - [mLDIFF-2])*vIIR + [mLDIFF-2]  ;R-to-L
        [mRDIFF] = (Rin + [dLDIFF]*vWALL - [mRDIFF-2])*vIIR + [mRDIFF-2]  ;L-to-R
        ___Early Echo (Comb Filter, with input from buffer)__________________________
        Lout=vCOMB1*[mLCOMB1]+vCOMB2*[mLCOMB2]+vCOMB3*[mLCOMB3]+vCOMB4*[mLCOMB4]
        Rout=vCOMB1*[mRCOMB1]+vCOMB2*[mRCOMB2]+vCOMB3*[mRCOMB3]+vCOMB4*[mRCOMB4]
        ___Late Reverb APF1 (All Pass Filter 1, with input from COMB)________________
        Lout=Lout-vAPF1*[mLAPF1-dAPF1], [mLAPF1]=Lout, Lout=Lout*vAPF1+[mLAPF1-dAPF1]
        Rout=Rout-vAPF1*[mRAPF1-dAPF1], [mRAPF1]=Rout, Rout=Rout*vAPF1+[mRAPF1-dAPF1]
        ___Late Reverb APF2 (All Pass Filter 2, with input from APF1)________________
        Lout=Lout-vAPF2*[mLAPF2-dAPF2], [mLAPF2]=Lout, Lout=Lout*vAPF2+[mLAPF2-dAPF2]
        Rout=Rout-vAPF2*[mRAPF2-dAPF2], [mRAPF2]=Rout, Rout=Rout*vAPF2+[mRAPF2-dAPF2]
        ___Output to Mixer (Output volume multiplied with input from APF2)___________
        LeftOutput  = Lout*vLOUT
        RightOutput = Rout*vROUT
        ___Finally, before repeating the above steps_________________________________
        BufferAddress = MAX(mBASE, (BufferAddress+2) AND 7FFFEh)
        Wait one 22050Hz cycle, then repeat the above stuff
        */

        // Push input samples into the FIR filter ring buffers
        self.input_buffer_l.push(dry_input.0 as i32);
        self.input_buffer_r.push(dry_input.1 as i32);

        // Filter the active FIR buffers to get the filtered input samples
        let filtered_input = if !self.odd_tick {
            self.input_buffer_l.filter()
        } else {
            self.input_buffer_r.filter()
        };

        let (vlin, dsame_tap, msame_dest, ddiff_tap, mdiff_dest, comb_addrs, apf1_addr, apf2_addr) =
            self.channel_fields();

        let input_sample: i32 = Self::mul(filtered_input, vlin);

        // Same-side reflection (left-to-left, right-to-right)
        self.reflection_stage(sound_ram, input_sample, msame_dest, dsame_tap);

        // Different-side reflection (left-to-right, right-to-left)
        self.reflection_stage(sound_ram, input_sample, mdiff_dest, ddiff_tap);

        // Early echo: sum four comb-filtered delay taps per channel
        let comb_output = self.comb_sum(
            sound_ram,
            comb_addrs,
            [self.vcomb1, self.vcomb2, self.vcomb3, self.vcomb4],
        );

        // Late reverb: two chained all-pass filter stages per channel
        let apf1_output =
            self.allpass_stage(sound_ram, comb_output, apf1_addr, self.dapf1, self.vapf1);
        let apf2_output: i32 =
            self.allpass_stage(sound_ram, apf1_output, apf2_addr, self.dapf2, self.vapf2);
        let output_sample = apf2_output.clamp(-0x8000, 0x7FFF);

        // Push to the correct output FIR buffer for filtering before output
        if !self.odd_tick {
            self.output_buffer_l.push(output_sample);
            self.output_buffer_r.push(0);
        } else {
            self.output_buffer_r.push(output_sample);
            self.output_buffer_l.push(0);
        }

        let filtered_l = (self.output_buffer_l.filter() << 1).clamp(-0x8000, 0x7FFF);
        let filtered_r = (self.output_buffer_r.filter() << 1).clamp(-0x8000, 0x7FFF);

        self.current_output = (
            PcmSample(Self::mul(filtered_l, self.vlout) as i16),
            PcmSample(Self::mul(filtered_r, self.vrout) as i16),
        );

        // Advance the circular buffer head by 1 sample (2 bytes), wrapping within the work area on odd ticks
        if !self.odd_tick {
            self.current_buffer_offset =
                (self.current_buffer_offset as usize + 2).rem_euclid(self.work_area_size()) as u32;
        }

        self.odd_tick = !self.odd_tick;
    }
}

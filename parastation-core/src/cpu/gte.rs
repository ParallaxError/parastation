/*
 * @file /parastation-core/src/cpu/gte.rs
 * @brief
 * Coprocessor 2 of the PS1, the Geometry Transformation Engine. Handles 3D graphics, and some
 * colour effects.
 *
 * https://problemkaputt.de/psx-spx.htm#geometrytransformationenginegte for reference
 *
 * -----
 */

// Imports
use crate::cpu::gte_div::gte_divide;

#[derive(Debug)]
pub struct GteRegister(pub u8);

// Helper structs for matrix/vector math
#[derive(Debug, Clone, Copy)]
pub struct Matrix(pub [[i32; 3]; 3]);
#[derive(Debug, Clone, Copy)]
pub struct Vector(pub [i32; 3]);

// State-agnostic helpers for GTE math
#[inline]
fn lsbs(value: u32) -> u16 {
    value as u16
}

#[inline]
fn msbs(value: u32) -> u16 {
    (value >> 16) as u16
}

fn saturate_to_ir(mac: i64, lm: bool) -> i16 {
    // Saturate IR1,IR2,IR3 result (0=To -8000h..+7FFFh, 1=To 0..+7FFFh)
    if lm {
        mac.clamp(0, 0x7FFF) as i16
    } else {
        mac.clamp(-0x8000, 0x7FFF) as i16
    }
}

// Honestly copied from Rustation when I was chasing a bug
// Looks like we also need to convert our 64 bit values to 44 bits sometimes
#[inline]
fn i64_to_i44(val: i64) -> i64 {
    (val << 20) >> 20
}

#[inline]
fn s32_to_i64(val: u32) -> i64 {
    val as i32 as i64
}

/// Truncate a signed 64-bit value to a signed 32-bit value, then sign-extend it back to 64 bits
#[inline]
fn truncate_i64_to_32(val: i64) -> i64 {
    val as i32 as i64
}

const MAC_MAX: i64 = 0x7FF_FFFF_FFFF; // 2^43 - 1
const MAC_MIN: i64 = -0x800_0000_0000; // -2^43

fn accumulate_mac_term(acc: i64, term: i64) -> (i64, bool, bool) {
    // Returns (wrapped_sum, positive_overflow, negative_overflow)
    let sum = acc + term;
    let pos = sum > MAC_MAX;
    let neg = sum < MAC_MIN;
    (i64_to_i44(sum), pos, neg)
}

// MAC (Matrix Accumulator) used by MVMVA and some others that use the translation + 3-term matrix multiply shape
fn matrix_vector_mac(
    matrix: &Matrix,
    vector: &Vector,
    translation: &Vector,
) -> ([i64; 3], [bool; 3], [bool; 3]) {
    /*
    Calculation:
    MAC1 = (Tx1*1000h + Mx11*Vx1 + Mx12*Vx2 + Mx13*Vx3) SAR (sf*12)
    MAC2 = (Tx2*1000h + Mx21*Vx1 + Mx22*Vx2 + Mx23*Vx3) SAR (sf*12)
    MAC3 = (Tx3*1000h + Mx31*Vx1 + Mx32*Vx2 + Mx33*Vx3) SAR (sf*12)
    */

    // Returns (mac, positive_overflow_occurred, negative_overflow_occurred) per row so caller can set flags
    const MAC_MAX: i64 = 0x7FF_FFFF_FFFF; // 2^43 - 1
    const MAC_MIN: i64 = -0x800_0000_0000; // -2^43

    let mut mac = [0i64; 3];
    let mut pos_overflow = [false; 3];
    let mut neg_overflow = [false; 3];

    for row in 0..3 {
        let (mut res, pos, neg) = accumulate_mac_term(0, translation.0[row] as i64 * 0x1000);
        pos_overflow[row] |= pos;
        neg_overflow[row] |= neg;

        for col in 0..3 {
            let term = matrix.0[row][col] as i64 * vector.0[col] as i64;
            let (new_res, pos, neg) = accumulate_mac_term(res, term);

            res = new_res;
            pos_overflow[row] |= pos;
            neg_overflow[row] |= neg;
        }

        mac[row] = res;
    }

    (mac, pos_overflow, neg_overflow)
}

fn matrix_vector_op(d: [i32; 3], ir: [i32; 3]) -> ([i64; 3], [bool; 3], [bool; 3]) {
    // Implements the Outer Product operation used by the OP command
    // Returns the 3 outer products and boolean flags for whether a positive or negative overflow occurred for each MAC
    /*
    [MAC1,MAC2,MAC3] = [IR3*D2-IR2*D3, IR1*D3-IR3*D1, IR2*D1-IR1*D2] SAR (sf*12)
    */

    // (pos_ir_idx, pos_d_idx, neg_ir_idx, neg_d_idx) per row matching psx-spx
    let terms: [(usize, usize, usize, usize); 3] = [
        (2, 1, 1, 2), // MAC1: IR3*D2 - IR2*D3
        (0, 2, 2, 0), // MAC2: IR1*D3 - IR3*D1
        (1, 0, 0, 1), // MAC3: IR2*D1 - IR1*D2
    ];

    let mut mac = [0i64; 3];
    let mut pos_overflow = [false; 3];
    let mut neg_overflow = [false; 3];

    for row in 0..3 {
        let (pi, pd, ni, nd) = terms[row];

        let (res, pos, neg) = accumulate_mac_term(0, ir[pi] as i64 * d[pd] as i64);
        pos_overflow[row] |= pos;
        neg_overflow[row] |= neg;

        let neg_term = ir[ni] as i64 * d[nd] as i64;
        let (res, pos, neg) = accumulate_mac_term(res, -neg_term);
        pos_overflow[row] |= pos;
        neg_overflow[row] |= neg;

        mac[row] = res;
    }

    (mac, pos_overflow, neg_overflow)
}

pub struct Gte {
    // 32 data registers, 32 control registers
    data: [u32; 32],
    control: [u32; 32],
}

impl Gte {
    pub fn new() -> Self {
        Self {
            data: [0; 32],
            control: [0; 32],
        }
    }
}

// Helpers
fn sign_extend_s16(s16: u32) -> u32 {
    // Truncate to 16 bits, then sign extend to 32 bits
    s16 as i16 as i32 as u32
}

pub fn saturate_i16(value: i32) -> i16 {
    value.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

// Named register read and writes
impl Gte {
    fn vz0(&self) -> u32 {
        sign_extend_s16(self.data[1])
    }

    fn vz1(&self) -> u32 {
        sign_extend_s16(self.data[3])
    }

    fn vz2(&self) -> u32 {
        sign_extend_s16(self.data[5])
    }

    fn ir0(&self) -> u32 {
        sign_extend_s16(self.data[8])
    }

    // All 3 are derived from IRGB (data[28])
    fn ir1(&self) -> u32 {
        sign_extend_s16(self.data[9])
    }

    fn ir2(&self) -> u32 {
        sign_extend_s16(self.data[10])
    }

    fn ir3(&self) -> u32 {
        sign_extend_s16(self.data[11])
    }

    /*
    The SZn Fifo has 4 stages (required for AVSZ4 command), the SXYn Fifo has only 3 stages, and a special mirrored
    register: SXYP is a mirror of SXY2, the difference is that writing to SXYP moves SXY2/SXY1 to SXY1/SXY0, whilst
    writing to SXY2 (or any other SXYn or SZn registers) changes only the written register, but doesn't move any other
    Fifo entries.
    */

    fn sxyp(&self) -> u32 {
        self.data[14]
    }

    fn set_sxyp(&mut self, value: u32) {
        // SXY0-SXY2 are registers 12-14
        self.data[12] = self.data[13];
        self.data[13] = self.data[14];
        self.data[14] = value;
    }

    fn set_ir123(&mut self, ir1: i16, ir2: i16, ir3: i16) {
        // Store as the real IR values to cache future reads
        self.data[9] = ir1 as u32;
        self.data[10] = ir2 as u32;
        self.data[11] = ir3 as u32;
        // Repack into IRGB's 15-bit form (clamped 0..0x1F per component)
        let r = ((ir1 as i32) / 0x80).clamp(0, 0x1F) as u32;
        let g = ((ir2 as i32) / 0x80).clamp(0, 0x1F) as u32;
        let b = ((ir3 as i32) / 0x80).clamp(0, 0x1F) as u32;
        self.data[28] = r | (g << 5) | (b << 10);
    }

    fn set_irgb(&mut self, value: u32) {
        /*
        Expands 5:5:5 bit RGB (range 0..1Fh) to 16:16:16 bit RGB (range 0000h..0F80h).
        0-4    Red   (0..1Fh) (R/W)  ;multiplied by 80h, and written to IR1
        5-9    Green (0..1Fh) (R/W)  ;multiplied by 80h, and written to IR2
        10-14  Blue  (0..1Fh) (R/W)  ;multiplied by 80h, and written to IR3
        15-31  Not used (always zero) (Read only)
        */

        let r = (value & 0x1F) * 0x80;
        let g = ((value >> 5) & 0x1F) * 0x80;
        let b = ((value >> 10) & 0x1F) * 0x80;
        self.set_ir123(r as i16, g as i16, b as i16);
    }

    fn orgb(&self) -> u32 {
        self.data[28] & 0x7FFF
    }

    fn lzcs(&self) -> u32 {
        self.data[30]
    }

    fn lzcr(&self) -> u32 {
        /*
        cop2r31 - LZCR - Count Leading Bits Result (R)
        Reading LZCR returns the leading 0 count of LZCS if LZCS is positive and the leading 1 count of LZCS if LZCS is
        negative. The results are in range 1..32.
        */

        let lzcs = self.lzcs();
        if lzcs == 0 {
            32
        } else if (lzcs & 0x8000_0000) != 0 {
            lzcs.leading_ones()
        } else {
            lzcs.leading_zeros()
        }
    }

    fn rt33(&self) -> u32 {
        sign_extend_s16(self.control[4])
    }

    fn l33(&self) -> u32 {
        sign_extend_s16(self.control[12])
    }

    fn lb33(&self) -> u32 {
        sign_extend_s16(self.control[20])
    }

    fn h_buggy(&self) -> u32 {
        /*
        The H value is 16bit unsigned (0bit sign, 16bit integer, 0bit fraction). BUG: When reading the H register, the
        hardware does accidently <sign-expand> the <unsigned> 16bit value (ie. values +8000h..+FFFFh are returned as
        FFFF8000h..FFFFFFFFh) (this bug applies only to "mov rd,cop2r58" opcodes; the actual calculations via RTPS/RTPT
        opcodes are working okay).
        */

        sign_extend_s16(self.control[26])
    }

    fn h(&self) -> u32 {
        self.control[26] & 0xFFFF
    }

    fn dqa(&self) -> u32 {
        sign_extend_s16(self.control[27])
    }

    fn zsf3(&self) -> u32 {
        sign_extend_s16(self.control[29])
    }

    fn zsf4(&self) -> u32 {
        sign_extend_s16(self.control[30])
    }

    fn flag(&self) -> u32 {
        // Bit 31 set if any error flags are set
        let masked_low = self.control[31] & !0xFFF;
        let err = ((masked_low >> 13) & 0x3F) | ((masked_low >> 23) & 0xFF);
        if err != 0 {
            masked_low | (1 << 31)
        } else {
            masked_low & !(1 << 31)
        }
    }
}

// Register access
impl Gte {
    pub fn read_register(&self, reg: GteRegister) -> u32 {
        match reg.0 {
            // Special cases first
            1 => self.vz0(),
            3 => self.vz1(),
            5 => self.vz2(),
            7 => self.data[7] & 0xFFFF, // OTZ is u16
            8 => self.ir0(),
            9 => self.ir1(),
            10 => self.ir2(),
            11 => self.ir3(),
            15 => self.sxyp(),
            16 => self.data[16] & 0xFFFF, // SZ0, u16
            17 => self.data[17] & 0xFFFF, // SZ1, u16
            18 => self.data[18] & 0xFFFF, // SZ2, u16
            19 => self.data[19] & 0xFFFF, // SZ3, u16
            28 => self.data[28] & 0x7FFF, // IRGB: U15, masked
            29 => self.orgb(),
            31 => self.lzcr(),
            36 => self.rt33(),
            44 => self.l33(),
            52 => self.lb33(),
            58 => self.h_buggy(),
            59 => self.dqa(),
            61 => self.zsf3(),
            62 => self.zsf4(),
            63 => self.flag(),
            // remaining 0-31 are data registers, 32-63 are control registers
            0..=31 => self.data[reg.0 as usize],
            32..=63 => self.control[(reg.0 - 32) as usize],
            _ => panic!("Invalid GTE register read: {}", reg.0),
        }
    }

    pub fn write_register(&mut self, reg: GteRegister, value: u32) {
        match reg.0 {
            // Special cases first
            9 => {
                let ir1 = value as i16;
                let ir2 = self.ir2() as i16;
                let ir3 = self.ir3() as i16;
                self.set_ir123(ir1, ir2, ir3);
            }
            10 => {
                let ir1 = self.ir1() as i16;
                let ir2 = value as i16;
                let ir3 = self.ir3() as i16;
                self.set_ir123(ir1, ir2, ir3);
            }
            11 => {
                let ir1 = self.ir1() as i16;
                let ir2 = self.ir2() as i16;
                let ir3 = value as i16;
                self.set_ir123(ir1, ir2, ir3);
            }
            15 => self.set_sxyp(value),
            28 => self.set_irgb(value),

            // remaining 0-31 are data registers, 32-63 are control registers
            0..=31 => self.data[reg.0 as usize] = value,
            32..=63 => self.control[(reg.0 - 32) as usize] = value,
            _ => panic!("Invalid GTE register write: {}", reg.0),
        }
    }

    pub fn execute_command(&mut self, command: u32) {
        /*
        GTE Command Encoding (COP2 imm25 opcodes)
        31-25  Must be 0100101b for "COP2 imm25" instructions
        20-24  Fake GTE Command Number (00h..1Fh) (ignored by hardware)
        19     sf - Shift Fraction in IR registers (0=No fraction, 1=12bit fraction)
        17-18  MVMVA Multiply Matrix    (0=Rotation. 1=Light, 2=Color, 3=Reserved)
        15-16  MVMVA Multiply Vector    (0=V0, 1=V1, 2=V2, 3=IR/long)
        13-14  MVMVA Translation Vector (0=TR, 1=BK, 2=FC/Bugged, 3=None)
        11-12  Always zero                        (ignored by hardware)
        10     lm - Saturate IR1,IR2,IR3 result (0=To -8000h..+7FFFh, 1=To 0..+7FFFh)
        6-9    Always zero                        (ignored by hardware)
        0-5    Real GTE Command Number (00h..3Fh) (used by hardware)
        */

        // Reset flags first
        self.control[31] = 0;

        let sf = (command & (1 << 19)) != 0;
        let lm = (command & (1 << 10)) != 0;
        let opcode = command & 0x3F;

        match opcode {
            0x01 => self.rtps(sf, lm),
            0x06 => self.nclip(),
            0x0C => self.op(sf, lm),
            0x10 => self.dpcs(sf, lm),
            0x11 => self.intpl(sf, lm),
            0x12 => {
                // Decode MVMA
                let mx = ((command >> 17) & 0x03) as u8;
                let vx = ((command >> 15) & 0x03) as u8;
                let tx = ((command >> 13) & 0x03) as u8;
                self.mvmva(sf, mx, vx, tx, lm);
            }
            0x13 => self.ncds(sf, lm),
            0x14 => self.cdp(sf, lm),
            0x16 => self.ncdt(sf, lm),
            0x1B => self.nccs(sf, lm),
            0x1C => self.cc(sf, lm),
            0x1E => self.ncs(sf, lm),
            0x20 => self.nct(sf, lm),
            0x28 => self.sqr(sf),
            0x29 => self.dcpl(sf, lm),
            0x2A => self.dpct(sf, lm),
            0x2D => self.avsz3(),
            0x2E => self.avsz4(),
            0x30 => self.rtpt(sf, lm),
            0x3D => self.gpf(sf, lm),
            0x3E => self.gpl(sf, lm),
            0x3F => self.ncct(sf, lm),
            _ => eprintln!("Unimplemented GTE command: {:02X}", opcode),
        }
    }
}

// State-aware helpers for the GTE
impl Gte {
    fn get_rt(&self) -> Matrix {
        Matrix([
            [
                lsbs(self.control[0]) as i16 as i32,
                msbs(self.control[0]) as i16 as i32,
                lsbs(self.control[1]) as i16 as i32,
            ],
            [
                msbs(self.control[1]) as i16 as i32,
                lsbs(self.control[2]) as i16 as i32,
                msbs(self.control[2]) as i16 as i32,
            ],
            [
                lsbs(self.control[3]) as i16 as i32,
                msbs(self.control[3]) as i16 as i32,
                self.rt33() as i32,
            ],
        ])
    }

    fn get_llm(&self) -> Matrix {
        Matrix([
            [
                lsbs(self.control[8]) as i16 as i32,
                msbs(self.control[8]) as i16 as i32,
                lsbs(self.control[9]) as i16 as i32,
            ],
            [
                msbs(self.control[9]) as i16 as i32,
                lsbs(self.control[10]) as i16 as i32,
                msbs(self.control[10]) as i16 as i32,
            ],
            [
                lsbs(self.control[11]) as i16 as i32,
                msbs(self.control[11]) as i16 as i32,
                self.l33() as i32,
            ],
        ])
    }

    fn get_lcm(&self) -> Matrix {
        Matrix([
            [
                lsbs(self.control[16]) as i16 as i32,
                msbs(self.control[16]) as i16 as i32,
                lsbs(self.control[17]) as i16 as i32,
            ],
            [
                msbs(self.control[17]) as i16 as i32,
                lsbs(self.control[18]) as i16 as i32,
                msbs(self.control[18]) as i16 as i32,
            ],
            [
                lsbs(self.control[19]) as i16 as i32,
                msbs(self.control[19]) as i16 as i32,
                self.lb33() as i32,
            ],
        ])
    }

    fn get_garbage_matrix(&self) -> Matrix {
        /*
        Mx=3 selects a garbage matrix (with elements -R*10h, +R*10h, IR0, RT13, RT13, RT13, RT22, RT22, RT22; whereas,
        R is LSB of RGBC register).
        */

        let rt = self.get_rt();
        let r = (self.data[6] & 0xFF) as i32;
        let rt13 = rt.0[0][2];
        let rt22 = rt.0[1][1];

        Matrix([
            [-r * 0x10, r * 0x10, self.ir0() as i32],
            [rt13, rt13, rt13],
            [rt22, rt22, rt22],
        ])
    }

    fn select_matrix(&self, mx: u8) -> Matrix {
        // For 0-2, RT/LLM/LCM
        // For 3:
        /*
        Mx=3 selects a garbage matrix (with elements -R*10h, +R*10h, IR0, RT13, RT13, RT13, RT22, RT22, RT22; whereas,
        R is LSB of RGBC register).
        */
        match mx {
            0 => self.get_rt(),
            1 => self.get_llm(),
            2 => self.get_lcm(),
            3 => self.get_garbage_matrix(),
            _ => unreachable!(),
        }
    }

    fn select_vector(&self, vx: u8) -> Vector {
        // 0-2 for V0-V2, 3 for IR
        match vx {
            0 => Vector([
                lsbs(self.data[0]) as i16 as i32,
                msbs(self.data[0]) as i16 as i32,
                self.vz0() as i32,
            ]),
            1 => Vector([
                lsbs(self.data[2]) as i16 as i32,
                msbs(self.data[2]) as i16 as i32,
                self.vz1() as i32,
            ]),
            2 => Vector([
                lsbs(self.data[4]) as i16 as i32,
                msbs(self.data[4]) as i16 as i32,
                self.vz2() as i32,
            ]),
            3 => Vector([self.ir1() as i32, self.ir2() as i32, self.ir3() as i32]),
            _ => unreachable!(),
        }
    }

    fn select_translation(&self, tx: u8) -> Vector {
        // 0 for TR, 1 for BK, 2 for bugged, 3 for none
        match tx {
            0 => Vector([
                self.control[5] as i32,
                self.control[6] as i32,
                self.control[7] as i32,
            ]),
            1 => Vector([
                self.control[13] as i32,
                self.control[14] as i32,
                self.control[15] as i32,
            ]),
            2 => Vector([
                self.control[21] as i32,
                self.control[22] as i32,
                self.control[23] as i32,
            ]),
            3 => Vector([0, 0, 0]),
            _ => unreachable!(),
        }
    }

    fn current_ir_i16(&self) -> [i16; 3] {
        [self.ir1() as i16, self.ir2() as i16, self.ir3() as i16]
    }

    fn check_and_saturate_ir(&mut self, ir_index: usize, value: i64, lm: bool) -> i16 {
        let (lo, hi) = if lm { (0, 0x7FFF) } else { (-0x8000, 0x7FFF) };
        if value < lo || value > hi {
            self.set_ir_saturation(ir_index);
        }
        saturate_to_ir(value, lm)
    }

    fn push_sz(&mut self, value: u32) {
        self.data[16] = self.data[17];
        self.data[17] = self.data[18];
        self.data[18] = self.data[19];
        self.data[19] = value;
    }

    fn push_rgb(&mut self, value: u32) {
        self.data[20] = self.data[21];
        self.data[21] = self.data[22];
        self.data[22] = value;
    }

    fn mac_multiply_add_saturate(
        &mut self,
        matrix: &Matrix,
        vector: &Vector,
        translation: &Vector,
        sf: bool,
        lm: bool,
    ) -> ([i64; 3], [i64; 3], [i16; 3]) {
        // Common helper for MVMVA and some other commands that use the MAC pipeline
        // The steps are
        // 1. RAW multiply-accumulate (MAC) calculation
        // 2. Overflow check on the RAW MAC value
        // 3. Apply the sf*12 shift
        // 4. Truncate to 32 bits, then IR-saturate
        // Returns (raw_mac, shifted_mac, ir) so the caller can use whichever it needs

        let (mac_raw, pos_overflow, neg_overflow) = matrix_vector_mac(matrix, vector, translation);

        // Set overflow flags from what the per-term accumulation already found
        for i in 0..3 {
            if pos_overflow[i] {
                self.set_mac_overflow(i, true);
            }
            if neg_overflow[i] {
                self.set_mac_overflow(i, false);
            }
        }

        // Apply the sf*12 shift
        let shift = if sf { 12 } else { 0 };
        let mac_shifted: [i64; 3] = [
            mac_raw[0] >> shift,
            mac_raw[1] >> shift,
            mac_raw[2] >> shift,
        ];

        // Truncate to 32 bits, then IR-saturate
        let mac32: [i64; 3] = [
            truncate_i64_to_32(mac_shifted[0]),
            truncate_i64_to_32(mac_shifted[1]),
            truncate_i64_to_32(mac_shifted[2]),
        ];

        let mut ir = [0i16; 3];
        for (i, &mac_val) in mac32.iter().enumerate() {
            ir[i] = self.check_and_saturate_ir(i + 1, mac_val, lm);
        }

        (mac_raw, mac_shifted, ir)
    }
}

// Flag raising helpers
impl Gte {
    /*
    cop2r63 (cnt31) - FLAG - Returns any calculation errors.
    31   Error Flag (Bit30..23, and 18..13 ORed together) (Read only)
    30   MAC1 Result positive 44bit overflow (max +7FFFFFFFFFFh) ;\triggered
    29   MAC2 Result positive 44bit overflow (max +7FFFFFFFFFFh) ; during
    28   MAC3 Result positive 44bit overflow (max +7FFFFFFFFFFh) ; calculations
    27   MAC1 Result negative 44bit overflow (min -80000000000h) ;
    26   MAC2 Result negative 44bit overflow (min -80000000000h) ;
    25   MAC3 Result negative 44bit overflow (min -80000000000h) ;/
    24   IR1 saturated to +0000h..+7FFFh (lm=1) or to -8000h..+7FFFh (lm=0)
    23   IR2 saturated to +0000h..+7FFFh (lm=1) or to -8000h..+7FFFh (lm=0)
    22   IR3 saturated to +0000h..+7FFFh (lm=1) or to -8000h..+7FFFh (lm=0)
    21   Color-FIFO-R saturated to +00h..+FFh
    20   Color-FIFO-G saturated to +00h..+FFh
    19   Color-FIFO-B saturated to +00h..+FFh
    18   SZ3 or OTZ saturated to +0000h..+FFFFh
    17   Divide overflow. RTPS/RTPT division result saturated to max=1FFFFh
    16   MAC0 Result positive 32bit overflow (max +7FFFFFFFh)    ;\triggered on
    15   MAC0 Result negative 32bit overflow (min -80000000h)    ;/final result
    14   SX2 saturated to -0400h..+03FFh
    13   SY2 saturated to -0400h..+03FFh
    12   IR0 saturated to +0000h..+1000h
    0-11 Not used (always zero) (Read only)
    */

    fn set_mac_overflow(&mut self, mac_index: usize, positive: bool) {
        let bit_index = match mac_index {
            0 => {
                if positive {
                    30
                } else {
                    27
                }
            }
            1 => {
                if positive {
                    29
                } else {
                    26
                }
            }
            2 => {
                if positive {
                    28
                } else {
                    25
                }
            }
            _ => unreachable!(),
        };

        self.control[31] |= 1 << bit_index;
    }

    fn check_mac0_overflow(&mut self, value: i64) {
        // MAC0 is 32-bit signed range
        const MAC0_MAX: i64 = (1i64 << 31) - 1;
        const MAC0_MIN: i64 = -(1i64 << 31);

        if value > MAC0_MAX {
            self.control[31] |= 1 << 16; // MAC0 positive 32bit overflow
        } else if value < MAC0_MIN {
            self.control[31] |= 1 << 15; // MAC0 negative 32bit overflow
        }
    }

    fn check_mac123_overflow(&mut self, mac_index: usize, value: i64) {
        if value > MAC_MAX {
            self.set_mac_overflow(mac_index, true);
        } else if value < MAC_MIN {
            self.set_mac_overflow(mac_index, false);
        }
    }

    fn set_ir_saturation(&mut self, ir_index: usize) {
        let bit_index = match ir_index {
            0 => 12,
            1 => 24,
            2 => 23,
            3 => 22,
            _ => unreachable!(),
        };

        self.control[31] |= 1 << bit_index;
    }

    fn set_division_overflow(&mut self) {
        self.control[31] |= 1 << 17;
    }
}

// GTE coordinate calculation commands
impl Gte {
    fn rtp_single(&mut self, sf: bool, lm: bool, vector_index: u8, is_last_vertex: bool) {
        /*
        COP2 0180001h - 15 Cycles - RTPS - Perspective Transformation (single)
        RTPS performs final Rotate, translate and perspective transformation on vertex V0. Before writing to the FIFOs,
        the older entries are moved one stage down. RTPT is same as RTPS, but repeats for V1 and V2. The "sf" bit should
        be usually set.
        IR1 = MAC1 = (TRX*1000h + RT11*VX0 + RT12*VY0 + RT13*VZ0) SAR (sf*12)
        IR2 = MAC2 = (TRY*1000h + RT21*VX0 + RT22*VY0 + RT23*VZ0) SAR (sf*12)
        IR3 = MAC3 = (TRZ*1000h + RT31*VX0 + RT32*VY0 + RT33*VZ0) SAR (sf*12)
        SZ3 = MAC3 SAR ((1-sf)*12)                           ;ScreenZ FIFO 0..+FFFFh
        MAC0=(((H*20000h/SZ3)+1)/2)*IR1+OFX, SX2=MAC0/10000h ;ScrX FIFO -400h..+3FFh
        MAC0=(((H*20000h/SZ3)+1)/2)*IR2+OFY, SY2=MAC0/10000h ;ScrY FIFO -400h..+3FFh
        MAC0=(((H*20000h/SZ3)+1)/2)*DQA+DQB, IR0=MAC0/1000h  ;Depth cueing 0..+1000h
        If the result of the "(((H*20000h/SZ3)+1)/2)" division is greater than 1FFFFh, then the division result is
        saturated to +1FFFFh, and the divide overflow bit in the FLAG register gets set; that happens if the vertex is
        exceeding the "near clip plane", ie. if it is very close to the camera (SZ3<=H/2), exactly at the camara
        position (SZ3=0), or behind the camera (negative Z coordinates are saturated to SZ3=0).
        */

        // This function uses vector_index so we can reuse it with 0 for RTPS, and 0-2 for RTPT

        // Select the TR and Vx vectors, and the RT matrix
        let translation = self.select_translation(0); // TR
        let vector = self.select_vector(vector_index);
        let matrix = self.select_matrix(0); // RT

        // Perform the matrix-vector multiplication and addition
        let (mac, pos_overflow, neg_overflow) = matrix_vector_mac(&matrix, &vector, &translation);

        // Set overflow flags from what the per-term accumulation already found
        for i in 0..3 {
            if pos_overflow[i] {
                self.set_mac_overflow(i, true);
            }
            if neg_overflow[i] {
                self.set_mac_overflow(i, false);
            }
        }

        // Now apply the sf*12 shift to get the actual stored MAC1-3
        let shift = if sf { 12 } else { 0 };
        let mac_shifted: [i64; 3] = [mac[0] >> shift, mac[1] >> shift, mac[2] >> shift];

        self.data[25] = mac_shifted[0] as u32;
        self.data[26] = mac_shifted[1] as u32;
        self.data[27] = mac_shifted[2] as u32;

        // Truncate to 32 bits
        let mac32: [i64; 3] = [
            truncate_i64_to_32(mac_shifted[0]),
            truncate_i64_to_32(mac_shifted[1]),
            truncate_i64_to_32(mac_shifted[2]),
        ];

        /*
        Note: The command does saturate IR1,IR2,IR3 to -8000h..+7FFFh (regardless of lm bit). When using RTP with sf=0,
        then the IR3 saturation flag (FLAG.22) gets set <only> if "MAC3 SAR 12" exceeds -8000h..+7FFFh (although IR3 is
        saturated when "MAC3" exceeds -8000h..+7FFFh).
        */
        let ir1 = self.check_and_saturate_ir(1, mac32[0], lm);
        let ir2 = self.check_and_saturate_ir(2, mac32[1], lm);

        // IR3 is special
        // The saturation FLAG is checked against MAC3 before truncation but with the sf*12 shift applied rather than
        // the truncated mac32[2], but the stored IR3 value still saturates mac32[2] normally
        let ir3_flag_check = mac[2] >> 12;
        if ir3_flag_check < -0x8000 || ir3_flag_check > 0x7FFF {
            self.set_ir_saturation(3);
        }
        let ir3 = saturate_to_ir(mac32[2], lm);

        self.set_ir123(ir1, ir2, ir3);

        // Set SZ3 to MAC3 SAR ((1-sf)*12)
        let sz3_unclamped = mac32[2] >> (if sf { 0 } else { 12 });

        // Check for clamping and set flags accordingly
        if sz3_unclamped < 0 || sz3_unclamped > 0xFFFF {
            self.control[31] |= 1 << 18; // SZ3 saturation
        }

        // SZ3 is really weird, but after bug chasing it seems it gets calculated from the RAW shifted value
        let sz3_unclamped = mac_shifted[2] >> (if sf { 0 } else { 12 });
        if sz3_unclamped < 0 || sz3_unclamped > 0xFFFF {
            self.control[31] |= 1 << 18;
        }
        let sz3 = sz3_unclamped.clamp(0, 0xFFFF);
        self.push_sz(sz3 as u32);

        // First common term we need is (((H*20000h/SZ3)+1)/2), and we need to check for divide overflow and sature if
        // so
        let (common_term, div_overflow) = gte_divide(self.h() as u16, sz3 as u16);

        // Don't like shadowing here... but it is convenient
        let common_term = common_term as i64;
        if div_overflow {
            self.set_division_overflow();
        }
        if sz3 == 0 {
            self.set_division_overflow(); // divide-by-zero is still its own overflow trigger
        }

        // Three calculations now for SX2, SY2 and IR0
        let mac0_sx2 = common_term * s32_to_i64(self.ir1()) + s32_to_i64(self.control[24]); // OFX
        let mac0_sy2 = common_term * s32_to_i64(self.ir2()) + s32_to_i64(self.control[25]); // OFY

        // Check for overflows
        self.check_mac0_overflow(mac0_sx2);
        self.check_mac0_overflow(mac0_sy2);

        // SXY2 is in register 14, so need to combine the two 16-bit values into one 32-bit value
        let sx2_raw = mac0_sx2 >> 16;
        let sy2_raw = mac0_sy2 >> 16;

        // Check for SX2/SY2 saturation, and set the flags if needed
        if sx2_raw < -0x400 || sx2_raw > 0x3FF {
            self.control[31] |= 1 << 14;
        }
        if sy2_raw < -0x400 || sy2_raw > 0x3FF {
            self.control[31] |= 1 << 13;
        }

        let sxy2 = (sx2_raw.clamp(-0x400, 0x3FF) as u32 & 0xFFFF)
            | ((sy2_raw.clamp(-0x400, 0x3FF) as u32 & 0xFFFF) << 16);
        // Push data by writing to SXYP, which will move SXY2/SXY1 to SXY1/SXY0, and write the new value to SXY2
        self.set_sxyp(sxy2);

        // Following block only happens if the vertex is the last one in the RTPT sequence, or if this is an RTPS
        // command
        if !is_last_vertex {
            return;
        }

        // IR0 calculation
        let mac0_ir0 = common_term * s32_to_i64(self.dqa()) + s32_to_i64(self.control[28]); // DQA + DQB
        self.check_mac0_overflow(mac0_ir0);

        // Finally set IR0 and MAC0 to the final depth cueing value
        let ir0_raw = mac0_ir0 >> 12;
        // Set error flag for saturation before clamping
        if ir0_raw < 0 || ir0_raw > 0x1000 {
            self.control[31] |= 1 << 12;
        }

        self.data[8] = ir0_raw.clamp(0, 0x1000) as u32;
        self.data[24] = mac0_ir0 as u32; // MAC0 is register 24
    }

    fn rtps(&mut self, sf: bool, lm: bool) {
        self.rtp_single(sf, lm, 0, true);
    }

    fn rtpt(&mut self, sf: bool, lm: bool) {
        for i in 0..3 {
            self.rtp_single(sf, lm, i, i == 2);
        }
    }

    fn nclip(&mut self) {
        /*
        COP2 1400006h - 8 Cycles - NCLIP - Normal clipping
        MAC0 = SX0*SY1 + SX1*SY2 + SX2*SY0 - SX0*SY2 - SX1*SY0 - SX2*SY1     ;slow
        Or, more efficient, with same result:
        MAC0 = SX0*(SY1-SY2) + SX1*(SY2-SY0) + SX2*(SY0-SY1)                 ;fast
        Error FLAG bit31,16,15 can get set if the final result exceeds 32bit range.
        */

        // cop2r12-15 6xS16 SXY0,SXY1,SXY2,SXYP, so we need to extract the 16-bit values from the 32-bit registers
        let sx0 = lsbs(self.data[12]) as i16 as i64;
        let sy0 = msbs(self.data[12]) as i16 as i64;
        let sx1 = lsbs(self.data[13]) as i16 as i64;
        let sy1 = msbs(self.data[13]) as i16 as i64;
        let sx2 = lsbs(self.data[14]) as i16 as i64;
        let sy2 = msbs(self.data[14]) as i16 as i64;

        let mac0 = sx0 * (sy1 - sy2) + sx1 * (sy2 - sy0) + sx2 * (sy0 - sy1);
        self.check_mac0_overflow(mac0);
        self.data[24] = mac0 as u32;
    }

    /*
    COP2 158002Dh - 5 Cycles - AVSZ3 - Average of three Z values (for Triangles)
    COP2 168002Eh - 6 Cycles - AVSZ4 - Average of four Z values (for Quads)
    MAC0 = ZSF3*(SZ1+SZ2+SZ3)       ;for AVSZ3
    MAC0 = ZSF4*(SZ0+SZ1+SZ2+SZ3)   ;for AVSZ4
    OTZ  = MAC0/1000h               ;for both (saturated to 0..FFFFh)
    Adds three or four Z values together and multiplies them by a fixed point value. The result can be used as index in
    the GPU's Ordering Table (OT).
    */

    fn avsz(&mut self, zsf: i64, szs: &[i64]) {
        let sum_sz: i64 = szs.iter().sum();
        let mac0 = zsf * sum_sz;
        self.check_mac0_overflow(mac0);
        self.data[24] = mac0 as u32;

        // OTZ = MAC0/1000h, saturated to 0..FFFFh
        let otz_raw = mac0 / 0x1000;
        if otz_raw < 0 || otz_raw > 0xFFFF {
            self.control[31] |= 1 << 18;
        }
        let otz = otz_raw.clamp(0, 0xFFFF) as u32;

        self.data[7] = otz;
    }

    fn avsz3(&mut self) {
        let szs = [
            (self.data[17] & 0xFFFF) as i64,
            (self.data[18] & 0xFFFF) as i64,
            (self.data[19] & 0xFFFF) as i64,
        ];
        let zsf3 = s32_to_i64(self.zsf3());
        self.avsz(zsf3, &szs);
    }

    fn avsz4(&mut self) {
        let szs = [
            (self.data[16] & 0xFFFF) as i64,
            (self.data[17] & 0xFFFF) as i64,
            (self.data[18] & 0xFFFF) as i64,
            (self.data[19] & 0xFFFF) as i64,
        ];
        let zsf4 = s32_to_i64(self.zsf4());
        self.avsz(zsf4, &szs);
    }
}

// GTE general purpose calculation commands
impl Gte {
    fn mvmva_fc_split_sum(
        &mut self,
        matrix: &Matrix,
        vector: &Vector,
        fc: &Vector,
        sf: bool,
        lm: bool,
    ) -> ([i64; 3], [i16; 3]) {
        // Ok, some fucked behaviour here (WARRANTED swear)
        /*
        Vx=2 selects the far color vector (FC), but this vector is not added correctly by the hardware: \
        The MAC calculation is split into two parts, part1 does merely affect the IR saturation flags and resets sum to
        0. For example, for MAC1 (and equivalent for MAC2,MAC3):
        IR1 = MAC1 = (Tx1*1000h + Mx11*Vx1) SAR (sf*12)  ;part1, saturate as if lm=0
        IR1 = MAC1 = (Mx12*Vx2 + Mx13*Vx3) SAR (sf*12)   ;part2, saturate by lm
        */
        // So this helper function does that calculation and returns the MAC and IR values, so the caller can use them
        // to set the registers and flags correctly

        let shift = if sf { 12 } else { 0 };

        for row in 0..3 {
            // Stage 1: Tx + Mx_col0 * V1, saturate as if lm=0, flags only, discard result
            let stage1 =
                (fc.0[row] as i64 * 0x1000 + matrix.0[row][0] as i64 * vector.0[0] as i64) >> shift;
            if stage1 > MAC_MAX {
                self.set_mac_overflow(row, true);
            } else if stage1 < MAC_MIN {
                self.set_mac_overflow(row, false);
            }

            let stage1_32 = truncate_i64_to_32(stage1);
            let _ = self.check_and_saturate_ir(row + 1, stage1_32, lm);
        }

        // Stage 2: Mx_col1*V2 + Mx_col2*V3
        let mut mac = [0i64; 3];
        for row in 0..3 {
            let stage2 = (matrix.0[row][1] as i64 * vector.0[1] as i64
                + matrix.0[row][2] as i64 * vector.0[2] as i64)
                >> shift;
            mac[row] = stage2;
        }

        let mac32: [i64; 3] = [
            truncate_i64_to_32(mac[0]),
            truncate_i64_to_32(mac[1]),
            truncate_i64_to_32(mac[2]),
        ];

        let mut ir = [0i16; 3];
        for (i, &mac_val) in mac32.iter().enumerate() {
            ir[i] = self.check_and_saturate_ir(i + 1, mac_val, lm);
        }

        (mac, ir)
    }

    fn mvmva(&mut self, sf: bool, mx: u8, vx: u8, tx: u8, lm: bool) {
        /*
        COP2 0400012h - 8 Cycles - MVMVA(sf,mx,v,cv,lm)
        Multiply vector by matrix and vector addition.
        Mx = matrix specified by mx  ;RT/LLM/LCM - Rotation, light or color matrix
        Vx = vector specified by v   ;V0, V1, V2, or [IR1,IR2,IR3]
        Tx = translation vector specified by cv  ;TR or BK or Bugged/FC, or None
        Calculation:
        MAC1 = (Tx1*1000h + Mx11*Vx1 + Mx12*Vx2 + Mx13*Vx3) SAR (sf*12)
        MAC2 = (Tx2*1000h + Mx21*Vx1 + Mx22*Vx2 + Mx23*Vx3) SAR (sf*12)
        MAC3 = (Tx3*1000h + Mx31*Vx1 + Mx32*Vx2 + Mx33*Vx3) SAR (sf*12)
        [IR1,IR2,IR3] = [MAC1,MAC2,MAC3]
        Multiplies a vector with either the rotation matrix, the light matrix or the color matrix and then adds the
        translation vector or background color vector.
        */

        let matrix = self.select_matrix(mx);
        let vector = self.select_vector(vx);

        let (mac, ir) = if tx == 2 {
            let fc = self.select_translation(2);
            self.mvmva_fc_split_sum(&matrix, &vector, &fc, sf, lm)
        } else {
            let translation = self.select_translation(tx);
            let (_, mac_shifted, ir) =
                self.mac_multiply_add_saturate(&matrix, &vector, &translation, sf, lm);
            (mac_shifted, ir)
        };

        self.set_ir123(ir[0], ir[1], ir[2]);

        self.data[25] = mac[0] as u32;
        self.data[26] = mac[1] as u32;
        self.data[27] = mac[2] as u32;
    }

    fn sqr(&mut self, sf: bool) {
        /*
        COP2 0A00428h+sf*80000h - 5 Cycles - SQR(sf) - Square vector
        [MAC1,MAC2,MAC3] = [IR1*IR1,IR2*IR2,IR3*IR3] SHR (sf*12)
        [IR1,IR2,IR3]    = [MAC1,MAC2,MAC3]    ;IR1,IR2,IR3 saturated to max 7FFFh
        Calculates the square of a vector. The result is, of course, always positive, so the "lm" flag for negative
        saturation has no effect.
        */

        let v = self.select_vector(3); // IR
        let ir_in = [v.0[0] as i64, v.0[1] as i64, v.0[2] as i64];

        let shift = if sf { 12 } else { 0 };
        let mut mac = [0i64; 3];
        for i in 0..3 {
            let raw = ir_in[i] * ir_in[i];
            self.check_mac123_overflow(i, raw);
            let wrapped = i64_to_i44(raw);
            mac[i] = wrapped >> shift;
        }

        self.data[25] = mac[0] as u32;
        self.data[26] = mac[1] as u32;
        self.data[27] = mac[2] as u32;

        let mac32 = [
            truncate_i64_to_32(mac[0]),
            truncate_i64_to_32(mac[1]),
            truncate_i64_to_32(mac[2]),
        ];

        // lm ignored since we can never get a negative result, so we always saturate to 0..7FFFh
        for (i, &m) in mac32.iter().enumerate() {
            if m < 0 || m > 0x7FFF {
                self.set_ir_saturation(i + 1);
            }
        }

        self.set_ir123(
            saturate_to_ir(mac32[0], true), // forced true regardless of lm param
            saturate_to_ir(mac32[1], true),
            saturate_to_ir(mac32[2], true),
        );
    }

    fn op(&mut self, sf: bool, lm: bool) {
        /*
        COP2 170000Ch+sf*80000h - 6 Cycles - OP(sf,lm) - Outer product of 2 vectors
        [MAC1,MAC2,MAC3] = [IR3*D2-IR2*D3, IR1*D3-IR3*D1, IR2*D1-IR1*D2] SAR (sf*12)
        [IR1,IR2,IR3]    = [MAC1,MAC2,MAC3]                        ;copy result
        Calculates the outer product of two signed 16bit vectors. Note: D1,D2,D3 are meant to be the RT11,RT22,RT33
        elements of the RT matrix "misused" as vector. lm should be usually zero.
        */

        let rt = self.select_matrix(0); // RT
        let d = [rt.0[0][0], rt.0[1][1], rt.0[2][2]];
        let ir_in = self.select_vector(3); // [IR1,IR2,IR3]

        let (mac_raw, pos_overflow, neg_overflow) = matrix_vector_op(d, ir_in.0);

        for i in 0..3 {
            if pos_overflow[i] {
                self.set_mac_overflow(i, true);
            }
            if neg_overflow[i] {
                self.set_mac_overflow(i, false);
            }
        }

        let shift = if sf { 12 } else { 0 };
        let mac_shifted: [i64; 3] = [
            mac_raw[0] >> shift,
            mac_raw[1] >> shift,
            mac_raw[2] >> shift,
        ];

        self.data[25] = mac_shifted[0] as u32;
        self.data[26] = mac_shifted[1] as u32;
        self.data[27] = mac_shifted[2] as u32;

        let mac32: [i64; 3] = [
            truncate_i64_to_32(mac_shifted[0]),
            truncate_i64_to_32(mac_shifted[1]),
            truncate_i64_to_32(mac_shifted[2]),
        ];

        let mut ir = [0i16; 3];
        for (i, &mac_val) in mac32.iter().enumerate() {
            ir[i] = self.check_and_saturate_ir(i + 1, mac_val, lm);
        }

        self.set_ir123(ir[0], ir[1], ir[2]);
    }
}

// GTE colour calculation commands
impl Gte {
    /*
    COP2 0C8041Eh - 14 Cycles - NCS - Normal color (single)
    COP2 0D80420h - 30 Cycles - NCT - Normal color (triple)
    COP2 108041Bh - 17 Cycles - NCCS - Normal Color Color (single vector)
    COP2 118043Fh - 39 Cycles - NCCT - Normal Color Color (triple vector)
    COP2 0E80413h - 19 Cycles - NCDS - Normal color depth cue (single vector)
    COP2 0F80416h - 44 Cycles - NCDT - Normal color depth cue (triple vectors)
    In: V0=Normal vector (for triple variants repeated with V1 and V2), BK=Background color, RGBC=Primary color/code,
    LLM=Light matrix, LCM=Color matrix, IR0=Interpolation value.
    [IR1,IR2,IR3] = [MAC1,MAC2,MAC3] = (LLM*V0) SAR (sf*12)
    [IR1,IR2,IR3] = [MAC1,MAC2,MAC3] = (BK*1000h + LCM*IR) SAR (sf*12)
    [MAC1,MAC2,MAC3] = [R*IR1,G*IR2,B*IR3] SHL 4          ;<--- for NCDx/NCCx
    [MAC1,MAC2,MAC3] = MAC+(FC-MAC)*IR0                   ;<--- for NCDx only
    [MAC1,MAC2,MAC3] = [MAC1,MAC2,MAC3] SAR (sf*12)       ;<--- for NCDx/NCCx
    Color FIFO = [MAC1/16,MAC2/16,MAC3/16,CODE], [IR1,IR2,IR3] = [MAC1,MAC2,MAC3]
    */

    // Some common helpers for the color commands
    fn light_transform(&mut self, v: &Vector, sf: bool, lm: bool) -> [i16; 3] {
        let llm = self.get_llm();
        let zero: Vector = Vector([0, 0, 0]);
        // LLM * V + 0, with SAR (sf*12)
        let (_, _, ir) = self.mac_multiply_add_saturate(&llm, v, &zero, sf, lm);
        ir
    }

    fn colour_transform(&mut self, sf: bool, lm: bool) -> ([i64; 3], [i16; 3]) {
        let lcm = self.get_lcm();
        let bk = self.select_translation(1); // BK
        let ir_in = self.select_vector(3); // IR
        // LCM * IR + BK: returns the raw MAC values, the caller can then apply the SAR (sf*12)
        let (mac_raw, _, ir) = self.mac_multiply_add_saturate(&lcm, &ir_in, &bk, sf, lm);
        (mac_raw, ir)
    }

    fn light_and_colour_transform(
        &mut self,
        v: &Vector,
        sf: bool,
        lm: bool,
    ) -> ([i64; 3], [i16; 3]) {
        // NCS, NCT, NCCS, NCCT, NCDS, NCDT does both light transform and colour transform
        // Quick helper to combine the two and return the MAC/IR values
        let ir1 = self.light_transform(v, sf, lm);
        self.set_ir123(ir1[0], ir1[1], ir1[2]);

        let (mac2, ir2) = self.colour_transform(sf, lm);
        self.set_ir123(ir2[0], ir2[1], ir2[2]);

        (mac2, ir2)
    }

    fn colour_transform_only(&mut self, sf: bool, lm: bool) -> ([i64; 3], [i16; 3]) {
        // CC/CDP directly does the colour transform, so we can skip the light transform step
        let (mac2, ir2) = self.colour_transform(sf, lm);
        self.set_ir123(ir2[0], ir2[1], ir2[2]);

        (mac2, ir2)
    }

    // Three helpers for the possible MAC1-3 calculations for colour commands, depending on the command variant

    fn shade_rgb(&self, rgb_word: u32, ir: [i16; 3]) -> [i64; 3] {
        // [MAC1,MAC2,MAC3] = [R*IR1,G*IR2,B*IR3] SHL 4
        let col = [
            (rgb_word & 0xFF) as i64,
            ((rgb_word >> 8) & 0xFF) as i64,
            ((rgb_word >> 16) & 0xFF) as i64,
        ];
        [
            i64_to_i44((col[0] * ir[0] as i64) << 4),
            i64_to_i44((col[1] * ir[1] as i64) << 4),
            i64_to_i44((col[2] * ir[2] as i64) << 4),
        ]
    }

    fn shade_rgb_flat(&self, rgb_word: u32) -> [i64; 3] {
        // shading = [R,G,B] << 16, used by DCPS/DPCT (omits IR multiplication)
        [
            ((rgb_word & 0xFF) as i64) << 16,
            (((rgb_word >> 8) & 0xFF) as i64) << 16,
            (((rgb_word >> 16) & 0xFF) as i64) << 16,
        ]
    }

    fn shade_ir_flat(&self, ir: [i16; 3]) -> [i64; 3] {
        // shading = [R,G,B] << 12, used by INTPL
        [
            (ir[0] as i64) << 12,
            (ir[1] as i64) << 12,
            (ir[2] as i64) << 12,
        ]
    }

    fn depth_cue_and_finish(&mut self, shading: [i64; 3], sf: bool, lm: bool) {
        // Ok, quite a loaded helper but a fairly shaded stage
        // It does (from psx-spx)
        // 1. [MAC1,2,3] = shading + (FC - shading) * IR0 SAR (sf*12)
        // 2. Push to RGB fifo and store IR1-3
        //      - Weird quirk, but FC*4096 - shading is 44 bit wrapped, shifted, and saturated to i16 as if lm=0 before
        //        multiplication

        let shift = if sf { 12 } else { 0 };
        let fc = [
            s32_to_i64(self.control[21]),
            s32_to_i64(self.control[22]),
            s32_to_i64(self.control[23]),
        ];
        let ir0 = s32_to_i64(self.ir0());

        let mut mac_sum = [0i64; 3];
        for i in 0..3 {
            let diff = (fc[i] << 12) - shading[i];
            self.check_mac123_overflow(i, diff);
            let tmp = i64_to_i44(diff) >> shift;
            let tmp32 = truncate_i64_to_32(tmp);
            if tmp32 < -0x8000 || tmp32 > 0x7FFF {
                self.set_ir_saturation(i + 1);
            }
            let saturated_tmp = saturate_to_ir(tmp32, false) as i64;
            mac_sum[i] = shading[i] + ir0 * saturated_tmp;
        }

        self.finish_colour(mac_sum, sf, lm);
    }

    fn finish_colour(&mut self, mac_sum: [i64; 3], sf: bool, lm: bool) {
        // Final stage shared by all colour commands:
        // [MAC1,MAC2,MAC3] = [MAC1,MAC2,MAC3] SAR (sf*12)       ;<--- for NCDx/NCCx
        // Color FIFO = [MAC1/16,MAC2/16,MAC3/16,CODE], [IR1,IR2,IR3] = [MAC1,MAC2,MAC3]
        let mut mac_raw = [0i64; 3];
        for i in 0..3 {
            self.check_mac123_overflow(i, mac_sum[i]);
            mac_raw[i] = i64_to_i44(mac_sum[i]);
        }

        let shift = if sf { 12 } else { 0 };
        let mac = [
            mac_raw[0] >> shift,
            mac_raw[1] >> shift,
            mac_raw[2] >> shift,
        ];

        self.data[25] = mac[0] as u32;
        self.data[26] = mac[1] as u32;
        self.data[27] = mac[2] as u32;

        let code = (self.data[6] >> 24) & 0xFF;
        let mac32 = [
            truncate_i64_to_32(mac[0]),
            truncate_i64_to_32(mac[1]),
            truncate_i64_to_32(mac[2]),
        ];

        let r_raw = mac32[0] >> 4;
        let g_raw = mac32[1] >> 4;
        let b_raw = mac32[2] >> 4;

        if r_raw < 0 || r_raw > 0xFF {
            self.control[31] |= 1 << 21;
        }
        if g_raw < 0 || g_raw > 0xFF {
            self.control[31] |= 1 << 20;
        }
        if b_raw < 0 || b_raw > 0xFF {
            self.control[31] |= 1 << 19;
        }

        let r = r_raw.clamp(0, 0xFF) as u32;
        let g = g_raw.clamp(0, 0xFF) as u32;
        let b = b_raw.clamp(0, 0xFF) as u32;
        self.push_rgb(r | (g << 8) | (b << 16) | (code << 24));

        // Check for saturation and set flags accordingly, then saturate to IR range and store in IR1-3
        let mut ir = [0i16; 3];
        for (i, &m) in mac32.iter().enumerate() {
            ir[i] = self.check_and_saturate_ir(i + 1, m, lm);
        }

        self.set_ir123(ir[0], ir[1], ir[2]);
    }

    // Finally commands just call the helpers above, with single or triple variants
    fn ncs_single(&mut self, sf: bool, lm: bool, vector_index: u8) {
        // NCS/NCT, light + colour transform, no shading, no depth-cue
        let (mac2, _) = self.light_and_colour_transform(&self.select_vector(vector_index), sf, lm);
        self.finish_colour(mac2, sf, lm);
    }

    fn ncs(&mut self, sf: bool, lm: bool) {
        self.ncs_single(sf, lm, 0);
    }

    fn nct(&mut self, sf: bool, lm: bool) {
        for i in 0..3 {
            self.ncs_single(sf, lm, i);
        }
    }

    fn nccs_single(&mut self, sf: bool, lm: bool, vector_index: u8) {
        // NCCS/NCCT, light + colour transform, shading, no depth-cue
        let (_, ir2) = self.light_and_colour_transform(&self.select_vector(vector_index), sf, lm);
        let shaded = self.shade_rgb(self.data[6], ir2);
        self.finish_colour(shaded, sf, lm);
    }

    fn nccs(&mut self, sf: bool, lm: bool) {
        self.nccs_single(sf, lm, 0);
    }

    fn ncct(&mut self, sf: bool, lm: bool) {
        for i in 0..3 {
            self.nccs_single(sf, lm, i);
        }
    }

    fn ncds_single(&mut self, sf: bool, lm: bool, vector_index: u8) {
        // NCDS/NCDT, light + colour transform, shading, depth-cue
        let (_, ir2) = self.light_and_colour_transform(&self.select_vector(vector_index), sf, lm);
        let shaded = self.shade_rgb(self.data[6], ir2);
        self.depth_cue_and_finish(shaded, sf, lm);
    }

    fn ncds(&mut self, sf: bool, lm: bool) {
        self.ncds_single(sf, lm, 0);
    }

    fn ncdt(&mut self, sf: bool, lm: bool) {
        for i in 0..3 {
            self.ncds_single(sf, lm, i);
        }
    }

    /*
    COP2 138041Ch - 11 Cycles - CC(lm=1) - Color Color
    COP2 1280414h - 13 Cycles - CDP(...) - Color Depth Que
    In: [IR1,IR2,IR3]=Vector, RGBC=Primary color/code, LCM=Color matrix, BK=Background color, and, for CDP,
    IR0=Interpolation value, FC=Far color.
    [IR1,IR2,IR3] = [MAC1,MAC2,MAC3] = (BK*1000h + LCM*IR) SAR (sf*12)
    [MAC1,MAC2,MAC3] = [R*IR1,G*IR2,B*IR3] SHL 4
    [MAC1,MAC2,MAC3] = MAC+(FC-MAC)*IR0                   ;<--- for CDP only
    [MAC1,MAC2,MAC3] = [MAC1,MAC2,MAC3] SAR (sf*12)
    Color FIFO = [MAC1/16,MAC2/16,MAC3/16,CODE], [IR1,IR2,IR3] = [MAC1,MAC2,MAC3]
    */
    fn cc(&mut self, sf: bool, lm: bool) {
        let (_, ir2) = self.colour_transform_only(sf, lm);
        let shaded = self.shade_rgb(self.data[6], ir2);
        self.finish_colour(shaded, sf, lm);
    }

    fn cdp(&mut self, sf: bool, lm: bool) {
        let (_, ir2) = self.colour_transform_only(sf, lm);
        let shaded = self.shade_rgb(self.data[6], ir2);
        self.depth_cue_and_finish(shaded, sf, lm);
    }

    /*
    COP2 0680029h - 8 Cycles - DCPL - Depth Cue Color light
    COP2 0780010h - 8 Cycles - DPCS - Depth Cueing (single)
    COP2 0x8002Ah - 17 Cycles - DPCT - Depth Cueing (triple)
    COP2 0980011h - 8 Cycles - INTPL - Interpolation of a vector and far color
    In: [IR1,IR2,IR3]=Vector, FC=Far Color, IR0=Interpolation value, CODE=MSB of RGBC, and, for DCPL, R,G,B=LSBs of
    RGBC.
    [MAC1,MAC2,MAC3] = [R*IR1,G*IR2,B*IR3] SHL 4          ;<--- for DCPL only
    [MAC1,MAC2,MAC3] = [IR1,IR2,IR3] SHL 12               ;<--- for INTPL only
    [MAC1,MAC2,MAC3] = [R,G,B] SHL 16                     ;<--- for DPCS/DPCT
    [MAC1,MAC2,MAC3] = MAC+(FC-MAC)*IR0
    [MAC1,MAC2,MAC3] = [MAC1,MAC2,MAC3] SAR (sf*12)
    Color FIFO = [MAC1/16,MAC2/16,MAC3/16,CODE], [IR1,IR2,IR3] = [MAC1,MAC2,MAC3]
    DPCT executes thrice, and reads the R,G,B values from RGB0 (ie. reads from the Bottom of the Color FIFO, instead of
    from the RGBC register) (the CODE value is kept read from RGBC as usually), so, after DPCT execution, the
    RGB0,RGB1,RGB2 Fifo entries are modified.
    */

    fn dcpl(&mut self, sf: bool, lm: bool) {
        // [MAC1,MAC2,MAC3] = [R*IR1,G*IR2,B*IR3] SHL 4, using current IR1-3
        let shaded = self.shade_rgb(self.data[6], self.current_ir_i16());
        self.depth_cue_and_finish(shaded, sf, lm);
    }

    fn dpcs(&mut self, sf: bool, lm: bool) {
        // [MAC1,MAC2,MAC3] = [R,G,B] SHL 16, from RGBC
        let shaded = self.shade_rgb_flat(self.data[6]);
        self.depth_cue_and_finish(shaded, sf, lm);
    }

    fn dpct(&mut self, sf: bool, lm: bool) {
        // Same as DPCS but executes thrice, reading R,G,B from the bottom of the colour FIFO (RGB0) each time, not from
        // RGBC. CODE still comes from RGBC.
        for _ in 0..3 {
            let shaded = self.shade_rgb_flat(self.data[20]); // RGB0
            self.depth_cue_and_finish(shaded, sf, lm);
        }
    }

    fn intpl(&mut self, sf: bool, lm: bool) {
        // [MAC1,MAC2,MAC3] = [IR1,IR2,IR3] SHL 12
        let shaded = self.shade_ir_flat(self.current_ir_i16());
        self.depth_cue_and_finish(shaded, sf, lm);
    }

    /*
    COP2 190003Dh - 5 Cycles - GPF(sf,lm) - General purpose Interpolation
    COP2 1A0003Eh - 5 Cycles - GPL(sf,?) - General Interpolation with base
    [MAC1,MAC2,MAC3] = [0,0,0]                            ;<--- for GPF only
    [MAC1,MAC2,MAC3] = [MAC1,MAC2,MAC3] SHL (sf*12)       ;<--- for GPL only
    [MAC1,MAC2,MAC3] = (([IR1,IR2,IR3] * IR0) + [MAC1,MAC2,MAC3]) SAR (sf*12)
    Color FIFO = [MAC1/16,MAC2/16,MAC3/16,CODE], [IR1,IR2,IR3] = [MAC1,MAC2,MAC3]
    Note: Although the SHL in GPL is theoretically undone by the SAR, 44bit overflows can occur internally when sf=1.
    */

    fn gpf(&mut self, sf: bool, lm: bool) {
        // [MAC1,MAC2,MAC3] = 0, then += IR*IR0, SAR(sf*12) happens inside finish_colour
        let ir = self.select_vector(3); // [IR1,IR2,IR3]
        let ir0 = s32_to_i64(self.ir0());

        let mut mac_sum = [0i64; 3];
        for i in 0..3 {
            let term = ir.0[i] as i64 * ir0;
            self.check_mac123_overflow(i, term);
            mac_sum[i] = i64_to_i44(term);
        }

        self.finish_colour(mac_sum, sf, lm);
    }

    fn gpl(&mut self, sf: bool, lm: bool) {
        // [MAC1,MAC2,MAC3] = current MAC SHL (sf*12), then += IR*IR0, SAR(sf*12) happens inside finish_colour
        let ir = self.select_vector(3); // [IR1,IR2,IR3]
        let ir0 = s32_to_i64(self.ir0());

        // Current MAC1-3, sign-extended from the stored 32-bit registers
        let base = [
            s32_to_i64(self.data[25]),
            s32_to_i64(self.data[26]),
            s32_to_i64(self.data[27]),
        ];

        let shift = if sf { 12 } else { 0 };

        let mut mac_sum = [0i64; 3];
        for i in 0..3 {
            let shifted_base = base[i] << shift;
            self.check_mac123_overflow(i, shifted_base); // Check overflow before wrapping to 44 bits
            let wrapped_base = i64_to_i44(shifted_base);

            let term = ir.0[i] as i64 * ir0;
            let sum = wrapped_base + term;
            self.check_mac123_overflow(i, sum);
            mac_sum[i] = i64_to_i44(sum);
        }

        self.finish_colour(mac_sum, sf, lm);
    }
}

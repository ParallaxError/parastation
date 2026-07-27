/*
 * @file /parastation-core/src/mdec.rs
 * @brief
 * PS1 Macroblock Decoder (MDEC) implementation. Handles decoding and decompression of media, used by some games for
 * cutscenes and FMVs.
 *
 * https://psx-spx.consoledev.net/macroblockdecodermdec/
 * https://github.com/simias/rustation-ng/blob/main/src/psx/mdec.rs
 * -----
 */

// Imports
use std::collections::VecDeque;

// https://psx-spx.consoledev.net/macroblockdecodermdec/#mdec-decompression
// Constant tables
// ^ this above is comments from when I was fruitlessly following psx-spx.
// The table below is shamelessly copied from rustation, and it works so whatever
const ZIGZAG: [usize; 64] = [
    0x00, 0x08, 0x01, 0x02, 0x09, 0x10, 0x18, 0x11, 0x0a, 0x03, 0x04, 0x0b, 0x12, 0x19,
    0x20, 0x28, 0x21, 0x1a, 0x13, 0x0c, 0x05, 0x06, 0x0d, 0x14, 0x1b, 0x22, 0x29, 0x30,
    0x38, 0x31, 0x2a, 0x23, 0x1c, 0x15, 0x0e, 0x07, 0x0f, 0x16, 0x1d, 0x24, 0x2b, 0x32,
    0x39, 0x3a, 0x33, 0x2c, 0x25, 0x1e, 0x17, 0x1f, 0x26, 0x2d, 0x34, 0x3b, 0x3c, 0x35,
    0x2e, 0x27, 0x2f, 0x36, 0x3d, 0x3e, 0x37, 0x3f,
];

#[derive(Debug, Copy, Clone)]
enum MdecCommand {
    DecodeMacroblock {
        depth: u8,
        signed: bool,
        bit15: bool,
        num_words: u16,
    },
    SetQuantTable {
        color: bool,
    },
    SetScaleTable,
    NoFunction,
}

/// PS1 Macroblock Decoder (MDEC) implementation. Handles decoding and decompression of media, used by some games for
/// cutscenes and FMVs.
pub struct Mdec {
    // Output metadata
    output_depth: u8,
    output_signed: bool,
    output_bit15: bool, // For 15 bit depth only

    // FIFOs
    pub data_out_fifo: VecDeque<u32>,
    data_in_fifo: VecDeque<u16>,

    // DMA
    data_in_request: bool,
    data_out_request: bool,
    dma_block_line_length: u8,
    dma_block_line: u8,
    dma_block_column: u8,

    // Decode and command state
    current_block: u8,
    param_words_remaining: u16, // FFFFh=None
    pending_command: MdecCommand,

    // Tables set by commands 2 and 3
    iq_y: [u8; 64],  // Luminance quant table
    iq_uv: [u8; 64], // Colour quant table
    scale_table: [i16; 64],

    // Per macroblock decode scratch
    cr_blk: [i32; 64],
    cb_blk: [i32; 64],
    y_blk: [i32; 64],
    rgb_out: [u8; 8 * 8 * 3], // One 8x8 quadrant at a time
}

impl Mdec {
    pub fn dma_block_line_length(&self) -> u8 {
        self.dma_block_line_length
    }

    pub fn new() -> Self {
        Self {
            output_depth: 0,
            output_signed: false,
            output_bit15: false,

            data_out_fifo: VecDeque::new(),
            data_in_fifo: VecDeque::new(),

            data_in_request: false,
            data_out_request: false,
            dma_block_line_length: 0,
            dma_block_line: 0,
            dma_block_column: 0,

            current_block: 4, // Matches JaCzekanski PS1 tests
            param_words_remaining: 0xFFFF,
            pending_command: MdecCommand::NoFunction,

            iq_y: [0; 64],
            iq_uv: [0; 64],
            scale_table: [0; 64],

            cr_blk: [0; 64],
            cb_blk: [0; 64],
            y_blk: [0; 64],
            rgb_out: [0; 8 * 8 * 3],
        }
    }

    // Register interface
    pub fn read_register(&mut self, offset: u32) -> u32 {
        match offset {
            0x00 => self.read_data_out(),
            0x04 => self.read_status(),
            _ => {
                eprintln!("Invalid MDEC register read: 0x{:08X}", offset);
                0
            }
        }
    }

    pub fn write_register(&mut self, offset: u32, value: u32) {
        match offset {
            0x00 => self.write_command_param(value),
            0x04 => self.write_control(value),
            _ => {
                eprintln!("Invalid MDEC register write: 0x{:08X}", offset)
            }
        }
    }

    fn read_data_out(&mut self) -> u32 {
        if let Some(value) = self.data_out_fifo.pop_front() {
            value
        } else {
            0
        }
    }

    /// Used only by MDEC to RAM DMA transfers. Returns the popped word plus the byte offset (relative to the 
    /// transfer's base address) it should be written to, since unlike other DMAs there is special interleaving 
    /// behaviour for the MDEC to RAM dma path
    pub fn dma_read_data_out(&mut self) -> (u32, u32) {
        let v = self.read_data_out();

        let line = self.dma_block_line as u32;
        let line_length = self.dma_block_line_length as u32;
        let mut offset = (line & 7) * line_length;
        if line & 8 != 0 {
            offset = offset.wrapping_sub(line_length * 7);
        }

        self.dma_block_column = self.dma_block_column.wrapping_sub(1);
        if self.dma_block_column == 0 {
            self.dma_block_column = self.dma_block_line_length;
            self.dma_block_line = self.dma_block_line.wrapping_add(1);
        }

        (v, offset << 2)
    }

    fn read_status(&self) -> u32 {
        /*
        31    Data-Out Fifo Empty (0=No, 1=Empty)
        30    Data-In Fifo Full   (0=No, 1=Full, or Last word received)
        29    Command Busy  (0=Ready, 1=Busy receiving or processing parameters)
        28    Data-In Request  (set when DMA0 enabled and ready to receive data)
        27    Data-Out Request (set when DMA1 enabled and ready to send data)
        26-25 Data Output Depth  (0=4bit, 1=8bit, 2=24bit, 3=15bit)      ;CMD.28-27
        24    Data Output Signed (0=Unsigned, 1=Signed)                  ;CMD.26
        23    Data Output Bit15  (0=Clear, 1=Set) (for 15bit depth only) ;CMD.25
        22-19 Not used (seems to be always zero)
        18-16 Current Block (0..3=Y1..Y4, 4=Cr, 5=Cb) (or for mono: always 4=Y)
        15-0  Number of Parameter Words remaining minus 1  (FFFFh=None)  ;CMD.Bit0-15
        */

        let mut status = 0;
        if self.data_out_fifo.is_empty() {
            status |= 1 << 31;
        }

        // Data in fifo always not full, since we instantly drain it when we get enough words

        if !matches!(self.pending_command, MdecCommand::NoFunction) {
            status |= 1 << 29;
        }

        if self.data_in_request {
            status |= 1 << 28;
        }

        if self.data_out_request && !self.data_out_fifo.is_empty() {
            status |= 1 << 27;
        }

        status |= (self.output_depth as u32) << 25;
        if self.output_signed {
            status |= 1 << 24;
        }
        if self.output_bit15 {
            status |= 1 << 23;
        }

        status |= (self.current_block as u32) << 16;
        if self.param_words_remaining != 0xFFFF {
            status |= (self.param_words_remaining as u32) - 1;
        } else {
            status |= 0xFFFF;
        }

        status
    }

    fn write_control(&mut self, value: u32) {
        /*
        31    Reset MDEC (0=No change, 1=Abort any command, and set status=80040000h)
        30    Enable Data-In Request  (0=Disable, 1=Enable DMA0 and Status.bit28)
        29    Enable Data-Out Request (0=Disable, 1=Enable DMA1 and Status.bit27)
        28-0  Unknown/Not used - usually zero
        */

        if value & (1 << 31) != 0 {
            // Turn self into what it is when constructed, kinda hacky
            *self = Self::new();
        }

        self.data_in_request = value & (1 << 30) != 0;
        self.data_out_request = value & (1 << 29) != 0;
    }

    // DMA methods
    pub fn wants_more_input(&self) -> bool {
        self.param_words_remaining != 0xFFFF
    }

    pub fn has_output_ready(&self) -> bool {
        !self.data_out_fifo.is_empty()
    }
}

// Commands and dispatch
impl Mdec {
    pub fn write_command_param(&mut self, value: u32) {
        // If we have no pending command, parse the command and persist, preparing for params
        if matches!(self.pending_command, MdecCommand::NoFunction) {
            self.output_depth = ((value >> 27) & 0b11) as u8;
            self.output_signed = (value & (1 << 26)) != 0;
            self.output_bit15 = (value & (1 << 25)) != 0;

            let cmd = ((value >> 29) & 0x7) as u8;
            match cmd {
                1 => {
                    /*
                    31-29 Command (1=decode_macroblock)
                    28-27 Data Output Depth  (0=4bit, 1=8bit, 2=24bit, 3=15bit)      ;STAT.26-25
                    26    Data Output Signed (0=Unsigned, 1=Signed)                  ;STAT.24
                    25    Data Output Bit15  (0=Clear, 1=Set) (for 15bit depth only) ;STAT.23
                    24-16 Not used (should be zero)
                    15-0  Number of Parameter Words (size of compressed data)
                    */

                    self.pending_command = MdecCommand::DecodeMacroblock {
                        depth: self.output_depth,
                        signed: self.output_signed,
                        bit15: self.output_bit15,
                        num_words: (value & 0xFFFF) as u16,
                    };
                    self.param_words_remaining = (value & 0xFFFF) as u16;
                }
                2 => {
                    /*
                    31-29 Command (2=set_iqtab)
                    28-1  Not used (should be zero)  ;Bit25-28 are copied to STAT.23-26 though
                    0     Color   (0=Luminance only, 1=Luminance and Color)
                    */

                    self.pending_command = MdecCommand::SetQuantTable {
                        color: (value & 1) != 0,
                    };
                    self.param_words_remaining = if (value & 1) != 0 { 32 } else { 16 };
                }
                3 => {
                    /*
                    31-29 Command (3=set_scale)
                    28-0  Not used (should be zero)  ;Bit25-28 are copied to STAT.23-26 though
                    */
                    self.pending_command = MdecCommand::SetScaleTable;
                    self.param_words_remaining = 32;
                }
                _ => {
                    // Unknown command, ignore
                    self.pending_command = MdecCommand::NoFunction;
                    self.param_words_remaining = 0xFFFF;
                }
            }

            // Its technically possible that the command has no parameters, in which case we can execute it immediately
            if self.param_words_remaining == 0 {
                self.param_words_remaining = 0xFFFF;
                self.execute_command();
            }

            return;
        }

        // If we fell through to here, we have a pending command and are receiving parameters so push to fifo
        self.data_in_fifo.push_back((value & 0xFFFF) as u16);
        self.data_in_fifo.push_back((value >> 16) as u16);
        self.param_words_remaining = self.param_words_remaining.saturating_sub(1);

        // If we've received all the parameters, execute the command
        if self.param_words_remaining == 0 {
            // Reset remaining param words to "none" so status shows FFFFh
            self.param_words_remaining = 0xFFFF;
            self.execute_command();
        }
    }

    fn execute_command(&mut self) {
        let command = self.pending_command;
        self.pending_command = MdecCommand::NoFunction;
        match command {
            MdecCommand::DecodeMacroblock {
                depth,
                signed,
                bit15,
                num_words,
            } => {
                self.decode_macroblock(depth, signed, bit15, num_words);
            }
            MdecCommand::SetQuantTable { color } => {
                // Should have either 64 or 128 words in the fifo, depending on color
                // 2 bytes packed into each param word, so need to extract
                // Set the luminance table regardless
                for i in (0..64).step_by(2) {
                    if let Some(word) = self.data_in_fifo.pop_front() {
                        self.iq_y[i] = (word & 0xFF) as u8;
                        self.iq_y[i + 1] = (word >> 8) as u8;
                    }
                }

                // If colour table is requested, set that too
                if color {
                    for i in (0..64).step_by(2) {
                        if let Some(word) = self.data_in_fifo.pop_front() {
                            self.iq_uv[i] = (word & 0xFF) as u8;
                            self.iq_uv[i + 1] = (word >> 8) as u8;
                        }
                    }
                }
            }
            MdecCommand::SetScaleTable => {
                for i in 0..64 {
                    if let Some(word) = self.data_in_fifo.pop_front() {
                        let coef = word as i16;
                        let shifted = coef >> 3;
                        let idx = ((i & 7) << 3) | ((i >> 3) & 7);
                        self.scale_table[idx] = shifted;
                    }
                }
            }
            MdecCommand::NoFunction => {
                // Do nothing
            }
        }
    }

    // https://psx-spx.consoledev.net/macroblockdecodermdec/#mdec-decompression
    fn decode_macroblock(&mut self, depth: u8, _signed: bool, _bit15: bool, _num_words: u16) {
        // Gonna keep parameters as unused for now, since could print those for debugging
        // kind of redundant though

        self.dma_block_line_length = match depth {
            0 | 1 => 0, // 4bpp/8bpp mono
            3 => 4,     // 15bpp
            2 => 6,     // 24bpp
            _ => 0,
        };
        self.dma_block_line = 0;
        self.dma_block_column = self.dma_block_line_length;

        self.current_block = 4;
        loop {
            // Either fifo is empty or everything left is padding, so no more macroblocks to decode
            if self.data_in_fifo.iter().all(|&w| w == 0xFE00) {
                break;
            }

            // For 15bpp or 24bpp depth (depth=2 or 3)
            if depth == 2 || depth == 3 {
                self.current_block = 4; // Cr
                Self::rl_decode_block(
                    &mut self.data_in_fifo,
                    &mut self.cr_blk,
                    &self.iq_uv,
                    &self.scale_table,
                );
                self.current_block = 5; // Cb
                Self::rl_decode_block(
                    &mut self.data_in_fifo,
                    &mut self.cb_blk,
                    &self.iq_uv,
                    &self.scale_table,
                );

                self.current_block = 0;
                Self::rl_decode_block(
                    &mut self.data_in_fifo,
                    &mut self.y_blk,
                    &self.iq_y,
                    &self.scale_table,
                );
                self.yuv_to_rgb(0, 0);
                self.flush_output(depth);

                self.current_block = 1;
                Self::rl_decode_block(
                    &mut self.data_in_fifo,
                    &mut self.y_blk,
                    &self.iq_y,
                    &self.scale_table,
                );
                self.yuv_to_rgb(8, 0);
                self.flush_output(depth);

                self.current_block = 2;
                Self::rl_decode_block(
                    &mut self.data_in_fifo,
                    &mut self.y_blk,
                    &self.iq_y,
                    &self.scale_table,
                );
                self.yuv_to_rgb(0, 8);
                self.flush_output(depth);

                self.current_block = 3;
                Self::rl_decode_block(
                    &mut self.data_in_fifo,
                    &mut self.y_blk,
                    &self.iq_y,
                    &self.scale_table,
                );
                self.yuv_to_rgb(8, 8);

                self.flush_output(depth);
            } else {
                self.current_block = 4;
                Self::rl_decode_block(
                    &mut self.data_in_fifo,
                    &mut self.y_blk,
                    &self.iq_y,
                    &self.scale_table,
                );
                self.y_to_mono();

                self.flush_output(depth);
            }
        }
    }

    fn signed10bit(value: u16) -> i32 {
        let mut signed = value as i32;
        if signed & 0x200 != 0 {
            signed |= !0x3FF; // Sign extend
        }
        signed
    }

    /// From rustation-ng, not sure why the psx-spx pseudocode just doesn't match... very annoying
    /// It oversimplifies the quantisation as val=(signed10bit(n AND 3FFh)*qt[k]*q_scale+4)/8, but rustation is a bit
    /// different so I just copied it. Credit given where credit is due!
    fn quantize(coef: u16, quantization: u8, qscale: Option<u8>) -> i16 {
        if coef == 0 {
            return 0;
        }

        let c = Self::signed10bit(coef) as i32;

        let (qs, qshift) = match qscale {
            Some(qs) => (qs as i32, 3),
            None => (1, 0),
        };

        let q = quantization as i32 * qs;

        let c_out = if q == 0 {
            c << 5
        } else {
            let c = (c * q) >> qshift;
            let c = c << 4;
            if c < 0 { c + 8 } else { c - 8 }
        };

        c_out.clamp(-0x4000, 0x3FFF) as i16
    }

    fn rl_decode_block(
        fifo: &mut VecDeque<u16>,
        block: &mut [i32; 64],
        quant_table: &[u8; 64],
        scale_table: &[i16; 64],
    ) {
        /*
        for i=0 to 63, blk[i]=0, next i   ;initially zerofill all entries (for skip)
        @@skip:
            n=[src], src=src+2, k=0           ;get first entry, init dest addr k=0
            if n=FE00h then @@skip            ;ignore padding (FE00h as first halfword)
            q_scale=(n SHR 10) AND 3Fh        ;contains scale value (not "skip" value)
            val=signed10bit(n AND 3FFh)*qt[k] ;calc first value (without q_scale/8) (?)
        @@lop:
            if q_scale=0 then val=signed10bit(n AND 3FFh)*2   ;special mode without qt[k]
            val=minmax(val,-400h,+3FFh)            ;saturate to signed 11bit range
            val=val*scalezag[i]                    ;<-- for "fast_idct_core" only
            if q_scale>0 then blk[zagzig[k]]=val   ;store entry (normal case)
            if q_scale=0 then blk[k]=val           ;store entry (special, no zigzag)
            n=[src], src=src+2                     ;get next entry (or FE00h end code)
            k=k+((n SHR 10) AND 3Fh)+1             ;skip zerofilled entries
            val=(signed10bit(n AND 3FFh)*qt[k]*q_scale+4)/8  ;calc value for next entry
            if k<=63 then jump @@lop          ;should end with n=FE00h (that sets k>63)
            idct_core(blk)
            return (with "src" address advanced)
        */

        // Now thats the actual psx-spx pseudocode but it gave really weird colours. I tried debugging for a bit,
        // but ultimatley just used rustation's implementaton so CREDIT WHERE CREDIT IS DUE.
        // Not sure why psx-spx has the wrong pseudocode, but ah well

        block.fill(0);
        let mut block_index: usize = 0;

        if fifo.is_empty() {
            Self::idct_core(block, scale_table);
            return;
        }

        loop {
            if block_index == 0 {
                let rle = match fifo.pop_front() {
                    Some(v) => v,
                    None => break,
                };
                if rle == 0xFE00 {
                    continue;
                }
                let q_scale = ((rle >> 10) & 0xFF) as u8;
                let dc = rle & 0x3FF;
                let dc_q = Self::quantize(dc, quant_table[0], None);
                block[ZIGZAG[0]] = dc_q as i32;
                block_index = 1;

                // AC loop from the rustation source, with q_scale applied to the quantization of each AC coefficient
                loop {
                    let rle = match fifo.pop_front() {
                        Some(v) => v,
                        None => break,
                    };
                    if rle == 0xFE00 {
                        while block_index < 64 {
                            block[ZIGZAG[block_index]] = 0;
                            block_index += 1;
                        }
                        break;
                    }
                    let zeroes = (rle >> 10) as usize;
                    let ac = rle & 0x3FF;
                    for _ in 0..zeroes {
                        if block_index >= 64 {
                            break;
                        }
                        block[ZIGZAG[block_index]] = 0;
                        block_index += 1;
                    }
                    if block_index < 64 {
                        let ac_q = Self::quantize(ac, quant_table[block_index], Some(q_scale));
                        block[ZIGZAG[block_index]] = ac_q as i32;
                        block_index += 1;
                    }
                    if block_index >= 64 {
                        break;
                    }
                }
                break;
            }
        }

        Self::idct_core(block, scale_table);
    }

    // Another thing that rustation does
    fn sign_extend_9bits_clamp_8bits(v: i32) -> i8 {
        let v = v as u16;
        let v = v << (16 - 9);
        let v = (v as i16) >> (16 - 9);
        v.clamp(-128, 127) as i8
    }

    fn idct_core(block: &mut [i32; 64], scale_table: &[i16; 64]) {
        /*
        src=blk, dst=temp_buffer
        for pass=0 to 1
            for x=0 to 7
            for y=0 to 7
                sum=0
                for z=0 to 7
                sum=sum+src[y+z*8]*(scaletable[x+z*8]/8)
                next z
                dst[x+y*8]=(sum+0fffh)/2000h               ;<-- or so?
            next y
            next x
            swap(src,dst)
        next pass
        */

        let mut block_tmp = [0i32; 64];
        // First pass: transpose write (x*8+y, not y*8+x) (idk why psx-spx wrong) but don't do output clamping yet
        for y in 0..8 {
            for x in 0..8 {
                let mut sum: i32 = 0;
                for c in 0..8 {
                    sum += block[y * 8 + c] * scale_table[x * 8 + c] as i32;
                }
                let v = (sum + 0x4000) >> 15;
                // Truncate to i16, matching rustation here
                block_tmp[x * 8 + y] = v as i16 as i32;
            }
        }

        // Second pass: normal write order then a clamp to signed 8 bit at the end
        for y in 0..8 {
            for x in 0..8 {
                let mut sum: i32 = 0;
                for c in 0..8 {
                    sum += block_tmp[y * 8 + c] * scale_table[x * 8 + c] as i32;
                }
                let v = (sum + 0x4000) >> 15;
                block[y * 8 + x] = Self::sign_extend_9bits_clamp_8bits(v) as i32;
            }
        }
    }

    fn yuv_to_rgb(&mut self, xx: usize, yy: usize) {
        /*
        for y=0 to 7
            for x=0 to 7
                R=[Crblk+((x+xx)/2)+((y+yy)/2)*8], B=[Cbblk+((x+xx)/2)+((y+yy)/2)*8]
                G=(-0.3437*B)+(-0.7143*R), R=(1.402*R), B=(1.772*B)
                Y=[Yblk+(x)+(y)*8]
                R=MinMax(-128,127,(Y+R))
                G=MinMax(-128,127,(Y+G))
                B=MinMax(-128,127,(Y+B))
                if unsigned then BGR=BGR xor 808080h  ;aka add 128 to the R,G,B values
                dst[(x+xx)+(y+yy)*16]=BGR
            next x
        next y
        */

        for y in 0..8 {
            for x in 0..8 {
                let cr = self.cr_blk[((x + xx) / 2) + ((y + yy) / 2) * 8];
                let cb = self.cb_blk[((x + xx) / 2) + ((y + yy) / 2) * 8];
                let y_val = self.y_blk[x + y * 8];

                let r = y_val + (((359 * cr) + 0x80) >> 8);
                let g = y_val + ((((-88 * cb) & !0x1f) + ((-183 * cr) & !0x07) + 0x80) >> 8);
                let b = y_val + (((454 * cb) + 0x80) >> 8);

                let mut r = Self::sign_extend_9bits_clamp_8bits(r) as u8;
                let mut g = Self::sign_extend_9bits_clamp_8bits(g) as u8;
                let mut b = Self::sign_extend_9bits_clamp_8bits(b) as u8;

                if !self.output_signed {
                    r ^= 0x80;
                    g ^= 0x80;
                    b ^= 0x80;
                }

                // Write into the 8x8-only scratch buffer, then software does the interleaving according to the tests
                let dst_index = (x + y * 8) * 3;
                self.rgb_out[dst_index] = b;
                self.rgb_out[dst_index + 1] = g;
                self.rgb_out[dst_index + 2] = r;
            }
        }
    }

    fn y_to_mono(&mut self) {
        /*
        for i=0 to 63
            Y=[Yblk+i]
            Y=Y AND 1FFh                  ;clip to signed 9bit range
            Y=MinMax(-128,127,Y)          ;saturate from 9bit to signed 8bit range
            if unsigned then Y=Y xor 80h  ;aka add 128 to the Y value
            dst[i]=Y
        next i
        */

        for i in 0..64 {
            let masked = self.y_blk[i] & 0x1FF;
            let mut y_val = if masked & 0x100 != 0 {
                masked - 0x200
            } else {
                masked
            };
            y_val = y_val.clamp(-128, 127);
            let mut y_out = y_val as u8;
            if !self.output_signed {
                y_out ^= 0x80;
            }
            self.rgb_out[i] = y_out;
        }
    }

    fn flush_output(&mut self, depth: u8) {
        let px_count = 64;

        match depth {
            2 => {
                // 24bpp, 3 bytes per pixel packed into 4 bytes per word
                for chunk in self.rgb_out[..px_count * 3].chunks(4) {
                    let mut buf = [0u8; 4];
                    buf[..chunk.len()].copy_from_slice(chunk);
                    self.data_out_fifo.push_back(u32::from_le_bytes(buf));
                }
            }
            3 => {
                // 15bpp, truncate each 8 bit chnanel to 5 bits and pack 2 pixels per word
                let mut pixels: Vec<u16> = Vec::with_capacity(px_count);
                for p in 0..px_count {
                    let r = self.rgb_out[p * 3 + 2];
                    let g = self.rgb_out[p * 3 + 1];
                    let b = self.rgb_out[p * 3];
                    let mut px = ((b as u16 >> 3) << 10) | ((g as u16 >> 3) << 5) | (r as u16 >> 3);
                    if self.output_bit15 {
                        px |= 0x8000;
                    }
                    pixels.push(px);
                }
                for pair in pixels.chunks(2) {
                    let lo = pair[0] as u32;
                    let hi = pair.get(1).copied().unwrap_or(0) as u32;
                    self.data_out_fifo.push_back(lo | (hi << 16));
                }
            }
            0 => {
                // 4bpp mono, top nybble of each byte packed into 1 byte per word
                for chunk in self.rgb_out[..64].chunks(8) {
                    let mut word = 0u32;
                    for (i, &b) in chunk.iter().enumerate() {
                        word |= ((b as u32 >> 4) & 0xF) << (i * 4);
                    }
                    self.data_out_fifo.push_back(word);
                }
            }
            1 => {
                // 8bpp mono, 4 pixels per word
                for chunk in self.rgb_out[..64].chunks(4) {
                    let mut buf = [0u8; 4];
                    buf[..chunk.len()].copy_from_slice(chunk);
                    self.data_out_fifo.push_back(u32::from_le_bytes(buf));
                }
            }
            _ => {
                eprintln!("Unsupported output depth: {}", depth);
            }
        }
    }
}

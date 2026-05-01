/*
 * @file /parastation-core/src/gpu/mod.rs
 * @brief
 * GPU struct that encapsulates platform-agnostic functionality of the PS1 GPU.
 * This includes the state encapsulation of the GPU and behaviour such as instruction decoding.
 * 
 * -----
 */

// Imports
mod gpu_state;
use gpu_state::{GpuState, DrawMode, TextureWindow, DrawingOffset};
pub use gpu_state::Mask;
mod rendering_parameters;
pub use rendering_parameters::*;
pub mod backend;
pub use backend::GpuBackend;
mod gpu_commands;
pub use gpu_commands::*;

/// Encapsulates the state and functionality of the PS1 GPU.
/// 
/// Absorbs register reads and writes when interacting with the GPU and owns the GPU backend
/// that draws to the desired output (e.g. a window or a framebuffer)
pub struct Gpu {
    state: GpuState,
    backend: Box<dyn GpuBackend>,

    // GP0 command accumulation
    gp0_buffer: Vec<u32>,
    gp0_words_remaining: u32,
}

impl Gpu {
    pub fn new(backend: Box<dyn GpuBackend>) -> Self {
        Self {
            state: GpuState::default(),
            backend,

            gp0_buffer: Vec::new(),
            gp0_words_remaining: 0,
        }
    }
}

// Register reads
impl Gpu {
    pub fn read_register(&self, offset: u32) -> u32 {
        match offset {
            0x00 => { eprintln!("GPUREAD register unimplemented"); 0 },
            0x04 => self.get_status_register(),
            _ => panic!("Invalid GPU register read offset: 0x{:02X}", offset)
        }
    }

    /// Get the 32 bit status register value based on the current state of the GPU.
    /// 
    /// https://problemkaputt.de/psx-spx.htm#gpustatusregister contains the bit layout of the 
    /// status register.
    fn get_status_register(&self) -> u32 {
        let mut status: u32 = 0;

        // Bit 0-3: texture page x base (draw mode)
        status |= (self.state.draw_mode.texture_base_x as u32) & 0xF;
        // Bit 4: texture page y base (draw mode)
        status |= (self.state.draw_mode.texture_base_y as u32) << 4;
        // Bit 5-6: semi transparency (draw mode)
        status |= (self.state.draw_mode.semi_transparency as u32) << 5;
        // Bit 7-8: texture page colour depth (draw mode)
        status |= (self.state.draw_mode.texture_page_colours as u32) << 7;
        // Bit 9: dither (draw mode)
        status |= (self.state.draw_mode.dither as u32) << 9;
        // Bit 10: draw to display (draw mode)
        status |= (self.state.draw_mode.draw_to_display as u32) << 10;

        // Bit 11: set bit mask when drawing (mask)
        status |= (self.state.mask.set_mask_while_drawing as u32) << 11;
        // Bit 12: check bit mask before drawing (mask)
        status |= (self.state.mask.check_mask_before_draw as u32) << 12;

        // Bit 13: interlace (display)
        status |= (self.state.display_state.vertical_interlace as u32) << 13;
        // Bit 14: Reverseflag (display)
        status |= (self.state.display_state.reverseflag as u32) << 14;
        
        // Bit 15: texture disable (draw mode)
        status |= (((self.state.draw_mode.texture_disable) && (self.state.display_state.texture_disable_allowed)) as u32) << 15;

        // Bit 16: horizontal resolution 2 (dplsay)
        status |= (self.state.display_state.horizontal_resolution_2 as u32) << 16;
        // Bit 17-18: horizontal resolution 1 (display)
        status |= (self.state.display_state.horizontal_resolution_1 as u32) << 17;
        // Bit 19: vertical resolution (display)
        status |= (self.state.display_state.vertical_resolution as u32) << 19;
        // Bit 20: video mode (display)
        status |= (self.state.display_state.video_mode as u32) << 20;
        // Bit 21: display colour depth (display)
        status |= (self.state.display_state.display_colour_depth as u32) << 21;
        // Bit 22: vertical interlace (display)
        status |= (self.state.display_state.vertical_interlace as u32) << 22;
        // Bit 23: display enable (display)
        status |= (self.state.display_state.display_enable as u32) << 23;

        // Bit 24: IRQ
        status |= (self.state.irq as u32) << 24;

        // Bit 25-28: based on DMA, just all 1s for now
        status |= 0xF << 25;

        // Bit 29-30: DMA direction (display)
        status |= (self.state.display_state.dma_direction as u32) << 29;

        // Bit 31: drawing even/odd lines in interlace mode (just stub 0), so nothing to do
        status
    }
}

// GP0 command handling and dispatch
impl Gpu {
    fn write_gp0(&mut self, word: u32) {
        // First, are we currently accumulating a command?
        if self.gp0_words_remaining > 0 {
            // Yes, so add this word to the buffer and decrease the count
            self.gp0_buffer.push(word);
            self.gp0_words_remaining -= 1;
        } else {
            // No, so this word is a new command. Decode it and set up the buffer and count.
            let command = decode_gp0_command(word);
            self.gp0_buffer.clear();
            self.gp0_buffer.push(word);
            self.gp0_words_remaining = gp0_command_parameter_count(&command) as u32;
        }

        // If we're done accumulating a command, execute it
        if self.gp0_words_remaining == 0 {
            let command = decode_gp0_command(self.gp0_buffer[0]);
            self.execute_gp0_command(command);

            // TODO should present with an interrupt, not unconditionally
            // self.display();
        }
    }
    
    fn execute_gp0_command(&mut self, command: Gp0Command) {
        match command {
            Gp0Command::MonochromeRectangle => self.draw_monochrome_rectangle(),

            Gp0Command::SetRenderingAttribute => self.set_rendering_attribute(),
            _ => eprintln!("GP0 command execution not implemented: {:?}", command),
        }
    }
}

// GP0 control commands
impl Gpu {
    fn set_rendering_attribute(&mut self) {
        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        match cmd {
            0xE1 => self.state.draw_mode = DrawMode::from_gp0_command(self.gp0_buffer[0]),
            0xE2 => self.state.texture_window = TextureWindow::from_gp0_command(self.gp0_buffer[0]),
            0xE3 => self.state.drawing_area.set_top_left(self.gp0_buffer[0]),
            0xE4 => self.state.drawing_area.set_bottom_right(self.gp0_buffer[0]),
            0xE5 => self.state.drawing_offset = DrawingOffset::from_gp0_command(self.gp0_buffer[0]),
            0xE6 => self.state.mask = Mask::from_gp0_command(self.gp0_buffer[0]),
            _ => eprintln!("Unknown GP0 rendering attribute command: 0x{:02X}", cmd)
        }
    }
}

// GP0 drawing commands
impl Gpu {
    fn display_width(&self) -> u16 {
        if self.state.display_state.horizontal_resolution_2 {
            368
        } else {
            match self.state.display_state.horizontal_resolution_1 {
                0 => 256,
                1 => 320,
                2 => 512,
                3 => 640,
                _ => unreachable!(),
            }
        }
    }

    fn display_height(&self) -> u16 {
        if self.state.display_state.vertical_resolution {
            480
        } else {
            240
        }
    }

    pub fn display(&mut self) {
        self.backend.present(
            self.state.display_state.display_start_x,
            self.state.display_state.display_start_y,
            self.display_width(),
            self.display_height(),
        );
    }

    fn get_draw_params(&self) -> DrawParams {
        DrawParams {
            drawing_area: self.state.drawing_area.clone(),
            drawing_offset: self.state.drawing_offset.clone(),
            mask: self.state.mask.clone(),
            draw_mode: self.state.draw_mode.clone(),
            semi_transparency: self.state.draw_mode.semi_transparency,
        }
    }

    fn draw_monochrome_rectangle(&mut self) {
        /*
        GP0(68h) - Monochrome Rectangle (1x1) (Dot) (opaque)
        GP0(6Ah) - Monochrome Rectangle (1x1) (Dot) (semi-transparent)
        GP0(70h) - Monochrome Rectangle (8x8) (opaque)
        GP0(72h) - Monochrome Rectangle (8x8) (semi-transparent)
        GP0(78h) - Monochrome Rectangle (16x16) (opaque)
        GP0(7Ah) - Monochrome Rectangle (16x16) (semi-transparent)
        1st  Color+Command     (CcBbGgRrh)
        2nd  Vertex            (YyyyXxxxh) 
         */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour = Colour::from_word(self.gp0_buffer[0]);
        let vertex = Vertex::from_word(self.gp0_buffer[1]);

        let semi_transparent = match cmd {
            0x68 | 0x70 | 0x78 => false,
            0x6A | 0x72 | 0x7A => true,
            _ => unreachable!(),
        };

        let size = match cmd {
            0x68 | 0x6A => RectSize::Fixed1x1,
            0x70 | 0x72 => RectSize::Fixed8x8,
            0x78 | 0x7A => RectSize::Fixed16x16,
            _ => unreachable!(),
        };

        let rect = Rect::Monochrome {
            colour,
            pos: vertex,
            size: size,
            semi_transparent: semi_transparent,
        };

        self.backend.draw_rect(&rect, &self.get_draw_params());
    }
}

// Register writes
impl Gpu {
    pub fn write_register(&mut self, offset: u32, value: u32) {
        match offset {
            0x00 => self.write_gp0(value),
            0x04 => self.write_gp1(value),
            _ => panic!("Invalid GPU register write offset: 0x{:02X}", offset)
        }
    }

    fn write_gp1(&mut self, word: u32) {
        let command_id = (word >> 24) as u8;
        match command_id {
            // Reset GPU
            0x00 => self.state.reset(),
            // Reset command buffer
            0x01 => { self.gp0_buffer.clear(); self.gp0_words_remaining = 0 },
            // Display enable
            0x03 => self.state.display_state.display_enable = (word & 0x1) != 0,
            // Set VRAM start 
            /* 
            0-9   X (0-1023)    (halfword address in VRAM)  (relative to begin of VRAM)
            10-18 Y (0-511)     (scanline number in VRAM)   (relative to begin of VRAM)
            19-23 Not used (zero)
             */
            0x05 => {
                let vram_x = (word & 0x3FF) as u16;
                let vram_y = ((word >> 10) & 0x1FF) as u16;
                self.state.display_state.display_start_x = vram_x;
                self.state.display_state.display_start_y = vram_y;
            },
            // Horizontal display range
            /*
            0-11   X1 (260h+0)       ;12bit       ;\counted in 53.222400MHz units,
            12-23  X2 (260h+320*8)   ;12bit       ;/relative to HSYNC
             */
            0x06 => {
                let x1 = (word & 0xFFF) as u16;
                let x2 = ((word >> 12) & 0xFFF) as u16;
                self.state.display_state.horizontal_range_x1 = x1;
                self.state.display_state.horizontal_range_x2 = x2;
            },
            // Vertical display range
            /*
            0-9   Y1 (NTSC=88h-(224/2), (PAL=A3h-(264/2))  ;\scanline numbers on screen,
            10-19 Y2 (NTSC=88h+(224/2), (PAL=A3h+(264/2))  ;/relative to VSYNC
            20-23 Not used (zero)
             */
            0x07 => {
                let y1 = (word & 0x3FF) as u16;
                let y2 = ((word >> 10) & 0x3FF) as u16;
                self.state.display_state.vertical_range_y1 = y1;
                self.state.display_state.vertical_range_y2 = y2;
            },
            // Display mode
            /*
            0-1   Horizontal Resolution 1     (0=256, 1=320, 2=512, 3=640) ;GPUSTAT.17-18
            2     Vertical Resolution         (0=240, 1=480, when Bit5=1)  ;GPUSTAT.19
            3     Video Mode                  (0=NTSC/60Hz, 1=PAL/50Hz)    ;GPUSTAT.20
            4     Display Area Color Depth    (0=15bit, 1=24bit)           ;GPUSTAT.21
            5     Vertical Interlace          (0=Off, 1=On)                ;GPUSTAT.22
            6     Horizontal Resolution 2     (0=256/320/512/640, 1=368)   ;GPUSTAT.16
            7     "Reverseflag"               (0=Normal, 1=Distorted)      ;GPUSTAT.14
            8-23  Not used (zero)
             */
            0x08 => {
                let horizontal_resolution_1 = ((word >> 2) & 0x3) as u8;
                let vertical_resolution = ((word >> 2) & 0x1) != 0;
                let video_mode = ((word >> 3) & 0x1) != 0;
                let display_colour_depth = ((word >> 4) & 0x1) != 0;
                let vertical_interlace = ((word >> 5) & 0x1) != 0;
                let horizontal_resolution_2 = ((word >> 6) & 0x1) != 0;
                let reverseflag = ((word >> 7) & 0x1) != 0;

                self.state.display_state.horizontal_resolution_1 = horizontal_resolution_1;
                self.state.display_state.vertical_resolution = vertical_resolution;
                self.state.display_state.video_mode = video_mode;
                self.state.display_state.display_colour_depth = display_colour_depth;
                self.state.display_state.vertical_interlace = vertical_interlace;
                self.state.display_state.horizontal_resolution_2 = horizontal_resolution_2;
                self.state.display_state.reverseflag = reverseflag;
            }
            _ => eprintln!("GP1 command not implemented: 0x{:02X}", command_id)
        }
    }
}
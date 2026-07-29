/*
 * @file /parastation-core/src/gpu/mod.rs
 * @brief
 * GPU struct that encapsulates platform-agnostic functionality of the PS1 GPU.
 * This includes the state encapsulation of the GPU and behaviour such as instruction decoding.
 *
 * -----
 */

// Imports
use std::cell::Cell;

mod gpu_state;
pub use gpu_state::Mask;
pub use gpu_state::{DisplayOutput, DrawMode, DrawingArea, DrawingOffset, GpuState, TextureWindow};
mod rendering_parameters;
pub use rendering_parameters::*;
pub mod backend;
pub use backend::GpuBackend;
mod gpu_commands;
pub use gpu_commands::*;

// VRAM transfer modes
enum Gp0Mode {
    Command,
    VramWrite,
    Polyline,
}

// GPUREAD mode
enum Gp0ReadMode {
    Vram,         // reading back VRAM via GP0(C0h)/Copy Rectangle
    GpuInfo(u32), // reading back GP1(10h) info, storing the requested sub-index
}

struct VramTransfer {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    current_x: u16,
    current_y: u16,
    words_remaining: u32,
}

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

    // HACK to avoid BIOS deadlock
    gpustat_bit31: Cell<bool>,

    // VRAM transfers
    gp0_mode: Gp0Mode,
    gp0_read_mode: Option<Gp0ReadMode>,
    gpuread_last_value: u32,
    vram_transfer: Option<VramTransfer>,
}

impl Gpu {
    pub fn new(backend: Box<dyn GpuBackend>) -> Self {
        Self {
            state: GpuState::default(),
            backend,

            gp0_buffer: Vec::new(),
            gp0_words_remaining: 0,
            gpustat_bit31: Cell::new(false),

            gp0_mode: Gp0Mode::Command,
            gp0_read_mode: None,
            gpuread_last_value: 0,
            vram_transfer: None,
        }
    }
}

// Register reads
impl Gpu {
    pub fn read_register(&mut self, offset: u32) -> u32 {
        match offset {
            0x00 => self.read_gpuread(),
            0x04 => self.get_status_register(),
            _ => panic!("Invalid GPU register read offset: 0x{:02X}", offset),
        }
    }

    fn read_gpuread(&mut self) -> u32 {
        match &self.gp0_read_mode {
            Some(Gp0ReadMode::Vram) => self.read_vram(),
            Some(Gp0ReadMode::GpuInfo(_)) => self.gpuread_last_value,
            None => self.gpuread_last_value, // Fallback if no command issued
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
        status |= (((self.state.draw_mode.texture_disable)
            && (self.state.display_state.texture_disable_allowed)) as u32)
            << 15;

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

        // Bit 31: drawing even/odd lines in interlace mode (just stub 0), gonna swap it each time to avoid deadlock
        let bit31 = !self.gpustat_bit31.get();
        self.gpustat_bit31.set(bit31);
        status |= (bit31 as u32) << 31;

        status
    }
}

// GP0 command handling and dispatch
impl Gpu {
    fn write_gp0(&mut self, word: u32) {
        match self.gp0_mode {
            // Just receive a command word for VRAM write mode
            Gp0Mode::VramWrite => {
                self.gp0_receive_vram_word(word);
                return;
            }

            Gp0Mode::Polyline => {
                self.gp0_buffer.push(word);

                // Per psx-spx: terminator is usually 0x55555555, but some games (Wild Arms 2) use 0x50005000
                // The reliable check is bits 12-15 and 28-31 both equal 0x5, ie. (word & 0xF000F000) == 0x50005000.
                if word & 0xF000_F000 == 0x5000_5000 {
                    self.gp0_mode = Gp0Mode::Command;
                    let command = decode_gp0_command(self.gp0_buffer[0]);
                    self.execute_gp0_command(command);
                }
                return;
            }

            Gp0Mode::Command => {
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
                    match command {
                        Gp0Command::MonochromePolyline | Gp0Command::ShadedPolyline => {
                            self.gp0_mode = Gp0Mode::Polyline;
                            return;
                        }
                        _ => {
                            self.gp0_words_remaining = gp0_command_parameter_count(&command) as u32;
                        }
                    }
                }

                // If we're done accumulating a command, execute it
                if self.gp0_words_remaining == 0 {
                    let command = decode_gp0_command(self.gp0_buffer[0]);
                    self.execute_gp0_command(command);
                }
            }
        }
    }

    fn execute_gp0_command(&mut self, command: Gp0Command) {
        match command {
            Gp0Command::Nop => {}
            Gp0Command::ClearCache => self.backend.clear_cache(),
            Gp0Command::FillRect => self.fill_rect(),

            Gp0Command::MonochromeTri => self.draw_monochrome_tri(),
            Gp0Command::MonochromeQuad => self.draw_monochrome_quad(),
            Gp0Command::TexturedTri => self.draw_textured_tri(),
            Gp0Command::TexturedQuad => self.draw_textured_quad(),
            Gp0Command::ShadedTri => self.draw_shaded_tri(),
            Gp0Command::ShadedQuad => self.draw_shaded_quad(),
            Gp0Command::ShadedTexturedTri => self.draw_shaded_textured_tri(),
            Gp0Command::ShadedTexturedQuad => self.draw_shaded_textured_quad(),

            Gp0Command::MonochromeLine => self.draw_monochrome_line(),
            Gp0Command::MonochromePolyline => self.draw_monochrome_polyline(),
            Gp0Command::ShadedLine => self.draw_shaded_line(),
            Gp0Command::ShadedPolyline => self.draw_shaded_polyline(),

            Gp0Command::VariableMonochromeRectangle => self.draw_variable_monochrome_rectangle(),
            Gp0Command::MonochromeRectangle => self.draw_monochrome_rectangle(),
            Gp0Command::VariableTexturedRectangle => self.draw_variable_textured_rectangle(),
            Gp0Command::TexturedRectangle => self.draw_textured_rectangle(),

            Gp0Command::SendRectToVram => self.begin_vram_write(),
            Gp0Command::CopyRectToCpu => self.begin_vram_read(),
            Gp0Command::CopyRect => self.copy_rect(),

            Gp0Command::SetRenderingAttribute => self.set_rendering_attribute(),
            _ => eprintln!(
                "GP0 command execution not implemented: {:?}, gp0_buffer: {:X?}",
                command, self.gp0_buffer
            ),
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
            _ => eprintln!("Unknown GP0 rendering attribute command: 0x{:02X}", cmd),
        }
    }
}

// GP0 VRAM commands
impl Gpu {
    fn begin_vram_read(&mut self) {
        /*
        1st  Command           (Cc000000h) ;\
        2nd  Source Coord      (YyyyXxxxh) ; write to GP0 port (as usually)
        3rd  Width+Height      (YsizXsizh) ;/
        ...  Data              (...)       ;<--- read from GPUREAD port (or via DMA)

        Transfers data from frame buffer to CPU. Wait for bit27 of the status register to be set before reading the
        image data. When the number of halfwords is odd, an extra halfword is read at the end (packets consist of
        32bit units).
        */

        let x = (self.gp0_buffer[1] & 0x3FF) as u16;
        let y = ((self.gp0_buffer[1] >> 16) & 0x1FF) as u16;
        let w = (self.gp0_buffer[2] & 0x3FF) as u16;
        let h = ((self.gp0_buffer[2] >> 16) & 0x1FF) as u16;

        if w == 0 || h == 0 {
            return;
        }

        self.backend.vram_read_begin(x, y, w, h);
        self.gp0_read_mode = Some(Gp0ReadMode::Vram);
    }

    fn read_vram(&mut self) -> u32 {
        // Read one word from VRAM, or 0 if no more left
        self.backend.vram_read().unwrap_or(0)
    }

    fn begin_vram_write(&mut self) {
        /*
        1st  Command           (Cc000000h)
        2nd  Destination Coord (YyyyXxxxh)
        3rd  Width+Height      (YsizXsizh)

        Transfers data from CPU to frame buffer.
        If the number of halfwords to be sent is odd, an extra halfword should be sent (packets consist of 32bit units).
        The transfer is affected by Mask setting.
        */

        let x = (self.gp0_buffer[1] & 0x3FF) as u16;
        let y = ((self.gp0_buffer[1] >> 16) & 0x1FF) as u16;
        let w = (self.gp0_buffer[2] & 0x3FF) as u16;
        let h = ((self.gp0_buffer[2] >> 16) & 0x1FF) as u16;

        // Word count is the total pixels rounded up to 32-bit words
        let pixel_count = w as u32 * h as u32;
        let words = (pixel_count + 1) / 2;

        if words == 0 {
            return;
        }

        self.vram_transfer = Some(VramTransfer {
            x,
            y,
            w,
            h,
            current_x: x,
            current_y: y,
            words_remaining: words,
        });

        self.gp0_words_remaining = 0; // We'll receive words until the count in vram_transfer is 0
        self.gp0_buffer.clear(); // Clear the buffer to receive the pixel data
        self.backend.vram_write_begin(x, y, w, h, &self.state.mask);
        self.gp0_mode = Gp0Mode::VramWrite;
    }

    fn gp0_receive_vram_word(&mut self, word: u32) {
        self.backend.vram_write(word);

        if let Some(transfer) = &mut self.vram_transfer {
            // Decrease the word count and check if we're done
            transfer.words_remaining -= 1;
            if transfer.words_remaining == 0 {
                self.gp0_mode = Gp0Mode::Command;
                self.vram_transfer = None;
            }
        }
    }

    fn fill_rect(&mut self) {
        /*
        1st  Color+Command     (CcBbGgRrh)  ;24bit RGB value (see note)
        2nd  Top Left Corner   (YyyyXxxxh)  ;Xpos counted in halfwords, steps of 10h
        3rd  Width+Height      (YsizXsizh)  ;Xsiz counted in halfwords, steps of 10h
        */

        let colour = Colour::from_word(self.gp0_buffer[0]);
        let pos = Vertex::from_word(self.gp0_buffer[1]);
        let w = (self.gp0_buffer[2] & 0x3FF) as u16;
        let h = ((self.gp0_buffer[2] >> 16) & 0x1FF) as u16;

        self.backend.fill_rect(pos, w, h, colour);
    }

    fn copy_rect(&mut self) {
        /*
        GP0(80h) - Copy Rectangle (VRAM to VRAM)
        1st  Command           (Cc000000h)
        2nd  Source Coord      (YyyyXxxxh)  ;Xpos counted in halfwords
        3rd  Destination Coord (YyyyXxxxh)  ;Xpos counted in halfwords
        4th  Width+Height      (YsizXsizh)  ;Xsiz counted in halfwords
        Copys data within framebuffer. The transfer is affected by Mask setting.
        Note: The command reads 128 halfwords from source to a temp buffer, then writes 128 halfwords from temp to
        dest; if the width isn't a multiple of 128 then the rightmost portion of each scanline will be less than
        128 halfwords).
        */

        let src_x = (self.gp0_buffer[1] & 0x3FF) as u16;
        let src_y = ((self.gp0_buffer[1] >> 16) & 0x1FF) as u16;
        let dst_x = (self.gp0_buffer[2] & 0x3FF) as u16;
        let dst_y = ((self.gp0_buffer[2] >> 16) & 0x1FF) as u16;
        let w = (self.gp0_buffer[3] & 0x3FF) as u16;
        let h = ((self.gp0_buffer[3] >> 16) & 0x1FF) as u16;

        self.backend
            .copy_rect(src_x, src_y, dst_x, dst_y, w, h, &self.state.mask);
    }
}

// GP0 drawing commands
impl Gpu {
    pub fn display(&mut self) {
        self.backend
            .present(&self.state.display_state.derive_output());
    }

    fn get_draw_params(&self) -> DrawParams {
        DrawParams {
            drawing_area: self.state.drawing_area.clone(),
            drawing_offset: self.state.drawing_offset.clone(),
            mask: self.state.mask.clone(),
            draw_mode: self.state.draw_mode.clone(),
            semi_transparency: self.state.draw_mode.semi_transparency,
            texture_window: self.state.texture_window.clone(),
        }
    }

    fn draw_monochrome_tri(&mut self) {
        /*
        GP0(20h) - Monochrome three-point polygon, opaque
        GP0(22h) - Monochrome three-point polygon, semi-transparent
        1st  Color+Command     (CcBbGgRrh)
        2nd  Vertex1           (YyyyXxxxh)
        3rd  Vertex2           (YyyyXxxxh)
        4th  Vertex3           (YyyyXxxxh)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour = Colour::from_word(self.gp0_buffer[0]);
        let vertex1 = Vertex::from_word(self.gp0_buffer[1]);
        let vertex2 = Vertex::from_word(self.gp0_buffer[2]);
        let vertex3 = Vertex::from_word(self.gp0_buffer[3]);

        let semi_transparent = match cmd {
            0x20 | 0x21 => false,
            0x22 | 0x23 => true,
            _ => unreachable!(),
        };

        let tri = Polygon::Monochrome {
            colour,
            vertices: PolygonVertices::Tri(
                FlatVertex { vertex: vertex1 },
                FlatVertex { vertex: vertex2 },
                FlatVertex { vertex: vertex3 },
            ),
            semi_transparent: semi_transparent,
        };

        self.backend.draw_polygon(&tri, &self.get_draw_params());
    }

    fn draw_monochrome_quad(&mut self) {
        /*
        GP0(28h) - Monochrome four-point polygon, opaque
        GP0(2Ah) - Monochrome four-point polygon, semi-transparent
        1st  Color+Command     (CcBbGgRrh)
        2nd  Vertex1           (YyyyXxxxh)
        3rd  Vertex2           (YyyyXxxxh)
        4th  Vertex3           (YyyyXxxxh)
        (5th) Vertex4           (YyyyXxxxh) (if any)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour = Colour::from_word(self.gp0_buffer[0]);
        let vertex1 = Vertex::from_word(self.gp0_buffer[1]);
        let vertex2 = Vertex::from_word(self.gp0_buffer[2]);
        let vertex3 = Vertex::from_word(self.gp0_buffer[3]);
        let vertex4 = Vertex::from_word(self.gp0_buffer[4]);

        let semi_transparent = match cmd {
            0x28 | 0x29 => false,
            0x2A | 0x2B => true,
            _ => unreachable!(),
        };

        let quad = Polygon::Monochrome {
            colour,
            vertices: PolygonVertices::Quad(
                FlatVertex { vertex: vertex1 },
                FlatVertex { vertex: vertex2 },
                FlatVertex { vertex: vertex3 },
                FlatVertex { vertex: vertex4 },
            ),
            semi_transparent: semi_transparent,
        };

        self.backend.draw_polygon(&quad, &self.get_draw_params());
    }

    fn draw_textured_tri(&mut self) {
        /*
        GP0(24h) - Textured three-point polygon, opaque, texture-blending
        GP0(25h) - Textured three-point polygon, opaque, raw-texture
        GP0(26h) - Textured three-point polygon, semi-transparent, texture-blending
        GP0(27h) - Textured three-point polygon, semi-transparent, raw-texture

        1st  Color+Command     (CcBbGgRrh) (color is ignored for raw-textures)
        2nd  Vertex1           (YyyyXxxxh)
        3rd  Texcoord1+Palette (ClutYyXxh)
        4th  Vertex2           (YyyyXxxxh)
        5th  Texcoord2+Texpage (PageYyXxh)
        6th  Vertex3           (YyyyXxxxh)
        7th  Texcoord3         (0000YyXxh)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour = Colour::from_word(self.gp0_buffer[0]);
        let vertex1 = Vertex::from_word(self.gp0_buffer[1]);
        let texcoord1 = Texcoord::from_word(self.gp0_buffer[2]);
        let vertex2 = Vertex::from_word(self.gp0_buffer[3]);
        let texcoord2 = Texcoord::from_word(self.gp0_buffer[4]);
        let vertex3 = Vertex::from_word(self.gp0_buffer[5]);
        let texcoord3 = Texcoord::from_word(self.gp0_buffer[6]);

        let semi_transparent = match cmd {
            0x24 | 0x25 => false,
            0x26 | 0x27 => true,
            _ => unreachable!(),
        };

        let raw_texture = match cmd {
            0x24 | 0x26 => false,
            0x25 | 0x27 => true,
            _ => unreachable!(),
        };

        let tex_page = TexPageAttr::from_word(self.gp0_buffer[4] >> 16);

        // Textured polygons implicitly update the persistent state, weird quirk
        // https://emudocs.layle.dev/PSX/Games/#fragmented-graphics
        self.state.draw_mode.texture_base_x = tex_page.x;
        self.state.draw_mode.texture_base_y = tex_page.y;
        self.state.draw_mode.semi_transparency = tex_page.semi_transparency;
        self.state.draw_mode.texture_page_colours = tex_page.colour_depth;

        let tri = Polygon::Textured {
            colour: colour,
            vertices: PolygonVertices::Tri(
                TexturedVertex {
                    vertex: vertex1,
                    texcoord: texcoord1,
                },
                TexturedVertex {
                    vertex: vertex2,
                    texcoord: texcoord2,
                },
                TexturedVertex {
                    vertex: vertex3,
                    texcoord: texcoord3,
                },
            ),
            semi_transparent: semi_transparent,
            texture_params: TextureParams {
                clut: Clut::from_word(self.gp0_buffer[2]),
                tex_page,
                raw_texture,
            },
        };

        self.backend.draw_polygon(&tri, &self.get_draw_params());
    }

    fn draw_textured_quad(&mut self) {
        /*
        GP0(2Ch) - Textured four-point polygon, opaque, texture-blending
        GP0(2Dh) - Textured four-point polygon, opaque, raw-texture
        GP0(2Eh) - Textured four-point polygon, semi-transparent, texture-blending
        GP0(2Fh) - Textured four-point polygon, semi-transparent, raw-texture
        1st  Color+Command     (CcBbGgRrh) (color is ignored for raw-textures)
        2nd  Vertex1           (YyyyXxxxh)
        3rd  Texcoord1+Palette (ClutYyXxh)
        4th  Vertex2           (YyyyXxxxh)
        5th  Texcoord2+Texpage (PageYyXxh)
        6th  Vertex3           (YyyyXxxxh)
        7th  Texcoord3         (0000YyXxh)
        (8th) Vertex4           (YyyyXxxxh) (if any)
        (9th) Texcoord4         (0000YyXxh) (if any)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour = Colour::from_word(self.gp0_buffer[0]);
        let vertex1 = Vertex::from_word(self.gp0_buffer[1]);
        let texcoord1 = Texcoord::from_word(self.gp0_buffer[2]);
        let vertex2 = Vertex::from_word(self.gp0_buffer[3]);
        let texcoord2 = Texcoord::from_word(self.gp0_buffer[4]);
        let vertex3 = Vertex::from_word(self.gp0_buffer[5]);
        let texcoord3 = Texcoord::from_word(self.gp0_buffer[6]);
        let vertex4 = Vertex::from_word(self.gp0_buffer[7]);
        let texcoord4 = Texcoord::from_word(self.gp0_buffer[8]);

        let semi_transparent = match cmd {
            0x2C | 0x2D => false,
            0x2E | 0x2F => true,
            _ => unreachable!(),
        };

        let raw_texture = match cmd {
            0x2C | 0x2E => false,
            0x2D | 0x2F => true,
            _ => unreachable!(),
        };

        let tex_page = TexPageAttr::from_word(self.gp0_buffer[4] >> 16);

        // Textured polygons implicitly update the persistent state, weird quirk
        // https://emudocs.layle.dev/PSX/Games/#fragmented-graphics
        self.state.draw_mode.texture_base_x = tex_page.x;
        self.state.draw_mode.texture_base_y = tex_page.y;
        self.state.draw_mode.semi_transparency = tex_page.semi_transparency;
        self.state.draw_mode.texture_page_colours = tex_page.colour_depth;

        let quad = Polygon::Textured {
            colour: colour,
            vertices: PolygonVertices::Quad(
                TexturedVertex {
                    vertex: vertex1,
                    texcoord: texcoord1,
                },
                TexturedVertex {
                    vertex: vertex2,
                    texcoord: texcoord2,
                },
                TexturedVertex {
                    vertex: vertex3,
                    texcoord: texcoord3,
                },
                TexturedVertex {
                    vertex: vertex4,
                    texcoord: texcoord4,
                },
            ),
            semi_transparent: semi_transparent,
            texture_params: TextureParams {
                clut: Clut::from_word(self.gp0_buffer[2]),
                tex_page: TexPageAttr::from_word(self.gp0_buffer[4] >> 16),
                raw_texture,
            },
        };

        self.backend.draw_polygon(&quad, &self.get_draw_params());
    }

    fn draw_shaded_tri(&mut self) {
        /*
        GP0(30h) - Shaded three-point polygon, opaque
        GP0(32h) - Shaded three-point polygon, semi-transparent
        1st  Color1+Command    (CcBbGgRrh)
        2nd  Vertex1           (YyyyXxxxh)
        3rd  Color2            (00BbGgRrh)
        4th  Vertex2           (YyyyXxxxh)
        5th  Color3            (00BbGgRrh)
        6th  Vertex3           (YyyyXxxxh)
        (7th) Color4            (00BbGgRrh) (if any)
        (8th) Vertex4           (YyyyXxxxh) (if any)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour1 = Colour::from_word(self.gp0_buffer[0]);
        let vertex1 = Vertex::from_word(self.gp0_buffer[1]);
        let colour2 = Colour::from_word(self.gp0_buffer[2]);
        let vertex2 = Vertex::from_word(self.gp0_buffer[3]);
        let colour3 = Colour::from_word(self.gp0_buffer[4]);
        let vertex3 = Vertex::from_word(self.gp0_buffer[5]);

        let semi_transparent = match cmd {
            0x30 | 0x31 => false,
            0x32 | 0x33 => true,
            _ => unreachable!(),
        };

        let tri = Polygon::Shaded {
            vertices: PolygonVertices::Tri(
                ShadedVertex {
                    vertex: vertex1,
                    colour: colour1,
                },
                ShadedVertex {
                    vertex: vertex2,
                    colour: colour2,
                },
                ShadedVertex {
                    vertex: vertex3,
                    colour: colour3,
                },
            ),
            semi_transparent: semi_transparent,
        };

        self.backend.draw_polygon(&tri, &self.get_draw_params());
    }

    fn draw_shaded_quad(&mut self) {
        /*
        GP0(38h) - Shaded four-point polygon, opaque
        GP0(3Ah) - Shaded four-point polygon, semi-transparent
        1st  Color1+Command    (CcBbGgRrh)
        2nd  Vertex1           (YyyyXxxxh)
        3rd  Color2            (00BbGgRrh)
        4th  Vertex2           (YyyyXxxxh)
        5th  Color3            (00BbGgRrh)
        6th  Vertex3           (YyyyXxxxh)
        (7th) Color4            (00BbGgRrh) (if any)
        (8th) Vertex4           (YyyyXxxxh) (if any)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour1 = Colour::from_word(self.gp0_buffer[0]);
        let vertex1 = Vertex::from_word(self.gp0_buffer[1]);
        let colour2 = Colour::from_word(self.gp0_buffer[2]);
        let vertex2 = Vertex::from_word(self.gp0_buffer[3]);
        let colour3 = Colour::from_word(self.gp0_buffer[4]);
        let vertex3 = Vertex::from_word(self.gp0_buffer[5]);
        let colour4 = Colour::from_word(self.gp0_buffer[6]);
        let vertex4 = Vertex::from_word(self.gp0_buffer[7]);

        let semi_transparent = match cmd {
            0x38 | 0x39 => false,
            0x3A | 0x3B => true,
            _ => unreachable!(),
        };

        let quad = Polygon::Shaded {
            vertices: PolygonVertices::Quad(
                ShadedVertex {
                    vertex: vertex1,
                    colour: colour1,
                },
                ShadedVertex {
                    vertex: vertex2,
                    colour: colour2,
                },
                ShadedVertex {
                    vertex: vertex3,
                    colour: colour3,
                },
                ShadedVertex {
                    vertex: vertex4,
                    colour: colour4,
                },
            ),
            semi_transparent: semi_transparent,
        };

        self.backend.draw_polygon(&quad, &self.get_draw_params());
    }

    fn draw_shaded_textured_tri(&mut self) {
        /*
        GP0(34h) - Shaded Textured three-point polygon, opaque, texture-blending
        GP0(36h) - Shaded Textured three-point polygon, semi-transparent, tex-blend
        1st  Color1+Command    (CcBbGgRrh)
        2nd  Vertex1           (YyyyXxxxh)
        3rd  Texcoord1+Palette (ClutYyXxh)
        4th  Color2            (00BbGgRrh)
        5th  Vertex2           (YyyyXxxxh)
        6th  Texcoord2+Texpage (PageYyXxh)
        7th  Color3            (00BbGgRrh)
        8th  Vertex3           (YyyyXxxxh)
        9th  Texcoord3         (0000YyXxh)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour1 = Colour::from_word(self.gp0_buffer[0]);
        let vertex1 = Vertex::from_word(self.gp0_buffer[1]);
        let texcoord1 = Texcoord::from_word(self.gp0_buffer[2]);
        let colour2 = Colour::from_word(self.gp0_buffer[3]);
        let vertex2 = Vertex::from_word(self.gp0_buffer[4]);
        let texcoord2 = Texcoord::from_word(self.gp0_buffer[5]);
        let colour3 = Colour::from_word(self.gp0_buffer[6]);
        let vertex3 = Vertex::from_word(self.gp0_buffer[7]);
        let texcoord3 = Texcoord::from_word(self.gp0_buffer[8]);

        let semi_transparent = match cmd {
            0x34 | 0x35 => false,
            0x36 | 0x37 => true,
            _ => unreachable!(),
        };

        let raw_texture = match cmd {
            0x34 | 0x36 => false,
            0x35 | 0x37 => true,
            _ => unreachable!(),
        };

        let tex_page = TexPageAttr::from_word(self.gp0_buffer[5] >> 16);

        // Textured polygons implicitly update the persistent state, weird quirk
        // https://emudocs.layle.dev/PSX/Games/#fragmented-graphics
        self.state.draw_mode.texture_base_x = tex_page.x;
        self.state.draw_mode.texture_base_y = tex_page.y;
        self.state.draw_mode.semi_transparency = tex_page.semi_transparency;
        self.state.draw_mode.texture_page_colours = tex_page.colour_depth;

        let tri = Polygon::ShadedTextured {
            vertices: PolygonVertices::Tri(
                ShadedTexturedVertex {
                    vertex: vertex1,
                    colour: colour1,
                    texcoord: texcoord1,
                },
                ShadedTexturedVertex {
                    vertex: vertex2,
                    colour: colour2,
                    texcoord: texcoord2,
                },
                ShadedTexturedVertex {
                    vertex: vertex3,
                    colour: colour3,
                    texcoord: texcoord3,
                },
            ),
            semi_transparent: semi_transparent,
            texture_params: TextureParams {
                clut: Clut::from_word(self.gp0_buffer[2]),
                tex_page,
                raw_texture: raw_texture,
            },
        };

        self.backend.draw_polygon(&tri, &self.get_draw_params());
    }

    fn draw_shaded_textured_quad(&mut self) {
        /*
        GP0(3Ch) - Shaded Textured four-point polygon, opaque, texture-blending
        GP0(3Eh) - Shaded Textured four-point polygon, semi-transparent, tex-blend
        1st  Color1+Command    (CcBbGgRrh)
        2nd  Vertex1           (YyyyXxxxh)
        3rd  Texcoord1+Palette (ClutYyXxh)
        4th  Color2            (00BbGgRrh)
        5th  Vertex2           (YyyyXxxxh)
        6th  Texcoord2+Texpage (PageYyXxh)
        7th  Color3            (00BbGgRrh)
        8th  Vertex3           (YyyyXxxxh)
        9th  Texcoord3         (0000YyXxh)
        (10th) Color4           (00BbGgRrh) (if any)
        (11th) Vertex4          (YyyyXxxxh) (if any)
        (12th) Texcoord4        (0000YyXxh) (if any)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour1 = Colour::from_word(self.gp0_buffer[0]);
        let vertex1 = Vertex::from_word(self.gp0_buffer[1]);
        let texcoord1 = Texcoord::from_word(self.gp0_buffer[2]);
        let colour2 = Colour::from_word(self.gp0_buffer[3]);
        let vertex2 = Vertex::from_word(self.gp0_buffer[4]);
        let texcoord2 = Texcoord::from_word(self.gp0_buffer[5]);
        let colour3 = Colour::from_word(self.gp0_buffer[6]);
        let vertex3 = Vertex::from_word(self.gp0_buffer[7]);
        let texcoord3 = Texcoord::from_word(self.gp0_buffer[8]);
        let colour4 = Colour::from_word(self.gp0_buffer[9]);
        let vertex4 = Vertex::from_word(self.gp0_buffer[10]);
        let texcoord4 = Texcoord::from_word(self.gp0_buffer[11]);

        let semi_transparent = match cmd {
            0x3C | 0x3D => false,
            0x3E | 0x3F => true,
            _ => unreachable!(),
        };

        let raw_texture = match cmd {
            0x3C | 0x3E => false,
            0x3D | 0x3F => true,
            _ => unreachable!(),
        };

        let tex_page = TexPageAttr::from_word(self.gp0_buffer[5] >> 16);

        // Textured polygons implicitly update the persistent state, weird quirk
        // https://emudocs.layle.dev/PSX/Games/#fragmented-graphics
        self.state.draw_mode.texture_base_x = tex_page.x;
        self.state.draw_mode.texture_base_y = tex_page.y;
        self.state.draw_mode.semi_transparency = tex_page.semi_transparency;
        self.state.draw_mode.texture_page_colours = tex_page.colour_depth;

        let quad = Polygon::ShadedTextured {
            vertices: PolygonVertices::Quad(
                ShadedTexturedVertex {
                    vertex: vertex1,
                    colour: colour1,
                    texcoord: texcoord1,
                },
                ShadedTexturedVertex {
                    vertex: vertex2,
                    colour: colour2,
                    texcoord: texcoord2,
                },
                ShadedTexturedVertex {
                    vertex: vertex3,
                    colour: colour3,
                    texcoord: texcoord3,
                },
                ShadedTexturedVertex {
                    vertex: vertex4,
                    colour: colour4,
                    texcoord: texcoord4,
                },
            ),
            semi_transparent: semi_transparent,
            texture_params: TextureParams {
                clut: Clut::from_word(self.gp0_buffer[2]),
                tex_page,
                raw_texture: raw_texture,
            },
        };

        self.backend.draw_polygon(&quad, &self.get_draw_params());
    }

    fn draw_monochrome_line(&mut self) {
        /*
         bit number   value   meaning
          31-29        010    line render
            25         1/0    semi-transparent / opaque
           23-0        rgb    first color value.

        1st   Color+Command     (CcBbGgRrh)
        2nd   Vertex1           (YyyyXxxxh)
        3rd   Vertex2           (YyyyXxxxh)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour = Colour::from_word(self.gp0_buffer[0]);
        let semi_transparent = cmd & 0x20 != 0;

        let vertex1 = Vertex::from_word(self.gp0_buffer[1]);
        let vertex2 = Vertex::from_word(self.gp0_buffer[2]);

        // Make two LinePoints
        let point1 = LinePoint { vertex: vertex1 };
        let point2 = LinePoint { vertex: vertex2 };

        let line = Line::Monochrome {
            colour: colour,
            vertices: vec![point1, point2],
            semi_transparent,
        };

        self.backend.draw_line(&line, &self.get_draw_params());
    }

    fn draw_monochrome_polyline(&mut self) {
        /*
         bit number   value   meaning
          31-29        011    line render
            25         1/0    semi-transparent / opaque
           23-0        rgb    first color value.

        1st   Color+Command     (CcBbGgRrh)
        2nd   Vertex1           (YyyyXxxxh)
        3rd   Vertex2           (YyyyXxxxh)
        (...)  VertexN           (YyyyXxxxh) (poly-line only)
        (Last) Termination Code  (55555555h) (poly-line only)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour = Colour::from_word(self.gp0_buffer[0]);
        let semi_transparent = cmd & 0x20 != 0;

        let make_vertex = |word: u32| -> LinePoint {
            let vertex = Vertex::from_word(word);
            LinePoint { vertex }
        };

        // Map 1:last in the buffer to vertices, stopping at last element
        let vertices: Vec<LinePoint> = self.gp0_buffer[1..self.gp0_buffer.len() - 1]
            .iter()
            .map(|&word| make_vertex(word))
            .collect();

        let line = Line::Monochrome {
            colour: colour,
            vertices,
            semi_transparent,
        };

        self.backend.draw_line(&line, &self.get_draw_params());
    }

    fn draw_shaded_line(&mut self) {
        /*
         bit number   value   meaning
          31-29        010    line render
            25         1/0    semi-transparent / opaque

        1st   Color1+Command    (CcBbGgRrh)
        2nd   Vertex1           (YyyyXxxxh)
        3rd   Color2            (00BbGgRrh)
        4th   Vertex2           (YyyyXxxxh)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let semi_transparent = cmd & 0x20 != 0;

        let colour1 = Colour::from_word(self.gp0_buffer[0]);
        let vertex1 = Vertex::from_word(self.gp0_buffer[1]);
        let colour2 = Colour::from_word(self.gp0_buffer[2]);
        let vertex2 = Vertex::from_word(self.gp0_buffer[3]);

        let point1 = ColouredLinePoint {
            vertex: vertex1,
            colour: colour1,
        };

        let point2 = ColouredLinePoint {
            vertex: vertex2,
            colour: colour2,
        };

        let line = Line::Coloured {
            vertices: vec![point1, point2],
            semi_transparent,
        };

        self.backend.draw_line(&line, &self.get_draw_params());
    }

    fn draw_shaded_polyline(&mut self) {
        /*
         bit number   value   meaning
          31-29        011    line render
            25         1/0    semi-transparent / opaque

        1st   Color1+Command    (CcBbGgRrh)
        2nd   Vertex1           (YyyyXxxxh)
        3rd   Color2            (00BbGgRrh)
        4th   Vertex2           (YyyyXxxxh)
        (...)  ColorN            (00BbGgRrh) (poly-line only)
        (...)  VertexN           (YyyyXxxxh) (poly-line only)
        (Last) Termination Code  (55555555h) (poly-line only)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let semi_transparent = cmd & 0x20 != 0;

        let make_vertex = |colour_word: u32, vertex_word: u32| -> ColouredLinePoint {
            let colour = Colour::from_word(colour_word);
            let vertex = Vertex::from_word(vertex_word);
            ColouredLinePoint { vertex, colour }
        };

        // Map 1:last in the buffer to vertices, stopping at last element
        let vertices: Vec<ColouredLinePoint> = (1..self.gp0_buffer.len() - 1)
            .step_by(2)
            .map(|i| make_vertex(self.gp0_buffer[i], self.gp0_buffer[i + 1]))
            .collect();

        let line = Line::Coloured {
            vertices,
            semi_transparent,
        };

        self.backend.draw_line(&line, &self.get_draw_params());
    }

    fn draw_variable_monochrome_rectangle(&mut self) {
        /*
        GP0(60h) - Monochrome Rectangle (variable size) (opaque)
        GP0(62h) - Monochrome Rectangle (variable size) (semi-transparent)
        1st  Color+Command     (CcBbGgRrh)
        2nd  Vertex            (YyyyXxxxh)
        (3rd) Width+Height      (YsizXsizh) (variable size only) (max 1023x511)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour = Colour::from_word(self.gp0_buffer[0]);
        let vertex = Vertex::from_word(self.gp0_buffer[1]);

        let semi_transparent = match cmd {
            0x60 => false,
            0x62 => true,
            _ => unreachable!(),
        };

        let size = if self.gp0_buffer.len() > 2 {
            let w = (self.gp0_buffer[2] & 0x3FF) as u16;
            let h = ((self.gp0_buffer[2] >> 16) & 0x1FF) as u16;
            RectSize::Variable { w: w, h: h }
        } else {
            RectSize::Fixed1x1 // Default just in case
        };

        let rect = Rect::Monochrome {
            colour,
            pos: vertex,
            size: size,
            semi_transparent: semi_transparent,
        };

        self.backend.draw_rect(&rect, &self.get_draw_params());
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

    fn draw_variable_textured_rectangle(&mut self) {
        /*
        GP0(64h) - Textured Rectangle, variable size, opaque, texture-blending
        GP0(65h) - Textured Rectangle, variable size, opaque, raw-texture
        GP0(66h) - Textured Rectangle, variable size, semi-transp, texture-blending
        GP0(67h) - Textured Rectangle, variable size, semi-transp, raw-texture
        1st  Color+Command     (CcBbGgRrh) (color is ignored for raw-textures)
        2nd  Vertex            (YyyyXxxxh) (upper-left edge of the rectangle)
        3rd  Texcoord+Palette  (ClutYyXxh) (see bug on odd/even Texcoord.X values)
        (4th) Width+Height      (YsizXsizh) (variable size only) (max 1023x511)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour = Colour::from_word(self.gp0_buffer[0]);
        let vertex = Vertex::from_word(self.gp0_buffer[1]);
        let texcoord = Texcoord::from_word(self.gp0_buffer[2]);

        let semi_transparent = match cmd {
            0x64 | 0x65 => false,
            0x66 | 0x67 => true,
            _ => unreachable!(),
        };

        let raw_texture = match cmd {
            0x64 | 0x66 => false,
            0x65 | 0x67 => true,
            _ => unreachable!(),
        };

        let size = if self.gp0_buffer.len() > 3 {
            let w = (self.gp0_buffer[3] & 0x3FF) as u16;
            let h = ((self.gp0_buffer[3] >> 16) & 0x1FF) as u16;
            RectSize::Variable { w: w, h: h }
        } else {
            RectSize::Fixed1x1 // Default just in case
        };

        let rect = Rect::Textured {
            colour,
            pos: vertex,
            size: size,
            texcoord,
            semi_transparent: semi_transparent,
            clut: Clut::from_word(self.gp0_buffer[2]),
            raw: raw_texture,
        };

        self.backend.draw_rect(&rect, &self.get_draw_params());
    }

    fn draw_textured_rectangle(&mut self) {
        /*
        GP0(6Ch) - Textured Rectangle, 1x1 (nonsense), opaque, texture-blending
        GP0(6Dh) - Textured Rectangle, 1x1 (nonsense), opaque, raw-texture
        GP0(6Eh) - Textured Rectangle, 1x1 (nonsense), semi-transp, texture-blending
        GP0(6Fh) - Textured Rectangle, 1x1 (nonsense), semi-transp, raw-texture
        GP0(74h) - Textured Rectangle, 8x8, opaque, texture-blending
        GP0(75h) - Textured Rectangle, 8x8, opaque, raw-texture
        GP0(76h) - Textured Rectangle, 8x8, semi-transparent, texture-blending
        GP0(77h) - Textured Rectangle, 8x8, semi-transparent, raw-texture
        GP0(7Ch) - Textured Rectangle, 16x16, opaque, texture-blending
        GP0(7Dh) - Textured Rectangle, 16x16, opaque, raw-texture
        GP0(7Eh) - Textured Rectangle, 16x16, semi-transparent, texture-blending
        GP0(7Fh) - Textured Rectangle, 16x16, semi-transparent, raw-texture
        1st  Color+Command     (CcBbGgRrh) (color is ignored for raw-textures)
        2nd  Vertex            (YyyyXxxxh) (upper-left edge of the rectangle)
        3rd  Texcoord+Palette  (ClutYyXxh) (see bug on odd/even Texcoord.X values)
        (4th) Width+Height      (YsizXsizh) (variable size only) (max 1023x511)
        */

        let cmd = (self.gp0_buffer[0] >> 24) as u8;
        let colour = Colour::from_word(self.gp0_buffer[0]);
        let vertex = Vertex::from_word(self.gp0_buffer[1]);
        let texcoord = Texcoord::from_word(self.gp0_buffer[2]);

        let semi_transparent = match cmd {
            0x6C | 0x6D | 0x74 | 0x75 | 0x7C | 0x7D => false,
            0x6E | 0x6F | 0x76 | 0x77 | 0x7E | 0x7F => true,
            _ => unreachable!(),
        };

        let raw_texture = match cmd {
            0x6C | 0x6E | 0x74 | 0x76 | 0x7C | 0x7E => false,
            0x6D | 0x6F | 0x75 | 0x77 | 0x7D | 0x7F => true,
            _ => unreachable!(),
        };

        let size = match cmd {
            0x6C | 0x6D | 0x6E | 0x6F => RectSize::Fixed1x1,
            0x74 | 0x75 | 0x76 | 0x77 => RectSize::Fixed8x8,
            0x7C | 0x7D | 0x7E | 0x7F => RectSize::Fixed16x16,
            _ => unreachable!(),
        };

        let rect = Rect::Textured {
            colour,
            pos: vertex,
            size: size,
            texcoord,
            semi_transparent: semi_transparent,
            clut: Clut::from_word(self.gp0_buffer[2]),
            raw: raw_texture,
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
            _ => panic!("Invalid GPU register write offset: 0x{:02X}", offset),
        }
    }

    fn write_gp1(&mut self, word: u32) {
        let command_id = (word >> 24) as u8;
        match command_id {
            // Reset GPU
            0x00 => self.state.reset(),
            // Reset command buffer
            0x01 => {
                self.gp0_buffer.clear();
                self.gp0_words_remaining = 0
            }
            // Acknowledge IRQ
            0x02 => self.state.irq = false,
            // Display enable
            0x03 => self.state.display_state.display_enable = (word & 0x1) != 0,
            // Set DMA direction
            0x04 => self.state.display_state.dma_direction = (word & 0x3) as u8,
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
            }
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
            }
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
            }
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
                let horizontal_resolution_1 = (word & 0x3) as u8; // bits 0-1
                let vertical_resolution = ((word >> 2) & 0x1) != 0; // bit 2
                let video_mode = ((word >> 3) & 0x1) != 0; // bit 3
                let display_colour_depth = ((word >> 4) & 0x1) != 0; // bit 4
                let vertical_interlace = ((word >> 5) & 0x1) != 0; // bit 5
                let horizontal_resolution_2 = ((word >> 6) & 0x1) != 0; // bit 6
                let reverseflag = ((word >> 7) & 0x1) != 0; // bit 7

                self.state.display_state.horizontal_resolution_1 = horizontal_resolution_1;
                self.state.display_state.vertical_resolution = vertical_resolution;
                self.state.display_state.video_mode = video_mode;
                self.state.display_state.display_colour_depth = display_colour_depth;
                self.state.display_state.vertical_interlace = vertical_interlace;
                self.state.display_state.horizontal_resolution_2 = horizontal_resolution_2;
                self.state.display_state.reverseflag = reverseflag;
            }

            0x10..=0x1F => self.gp1_get_gpu_info(word & 0xFF),
            _ => eprintln!("GP1 command not implemented: 0x{:02X}", command_id),
        }
    }

    fn gp1_get_gpu_info(&mut self, sub_index: u32) {
        /*
        GP1(10h) - Get GPU Info
        GP1(11h..1Fh) - Mirrors of GP1(10h), Get GPU Info
        After sending the command, the result can be immediately read from GPUREAD register (there's no NOP or other
        delay required) (namely GPUSTAT.Bit27 is used only for VRAM-Reads, but NOT for GPU-Info-Reads, so do not try
        to wait for that flag).
        0-23  Select Information which is to be retrieved (via following GPUREAD)
        On Old 180pin GPUs, following values can be selected:
        00h-01h = Returns Nothing (old value in GPUREAD remains unchanged)
        02h     = Read Texture Window setting  ;GP0(E2h) ;20bit/MSBs=Nothing
        03h     = Read Draw area top left      ;GP0(E3h) ;19bit/MSBs=Nothing
        04h     = Read Draw area bottom right  ;GP0(E4h) ;19bit/MSBs=Nothing
        05h     = Read Draw offset             ;GP0(E5h) ;22bit
        06h-07h = Returns Nothing (old value in GPUREAD remains unchanged)
        08h-FFFFFFh = Mirrors of 00h..07h
        On New 208pin GPUs, following values can be selected:
        00h-01h = Returns Nothing (old value in GPUREAD remains unchanged)
        02h     = Read Texture Window setting  ;GP0(E2h) ;20bit/MSBs=Nothing
        03h     = Read Draw area top left      ;GP0(E3h) ;20bit/MSBs=Nothing
        04h     = Read Draw area bottom right  ;GP0(E4h) ;20bit/MSBs=Nothing
        05h     = Read Draw offset             ;GP0(E5h) ;22bit
        06h     = Returns Nothing (old value in GPUREAD remains unchanged)
        07h     = Read GPU Type (usually 2)    ;see "GPU Versions" chapter
        08h     = Unknown (Returns 00000000h) (lightgun on some GPUs?)
        09h-0Fh = Returns Nothing (old value in GPUREAD remains unchanged)
        10h-FFFFFFh = Mirrors of 00h..0Fh
        The selected data is latched in GPUREAD, the same/latched value can be read multiple times, but, the latch
        isn't automatically updated when changing GP0 registers.
        */

        let value = match sub_index & 0xF {
            0x2 => {
                // Texture window setting, packed back into GP0(E2h) format
                let tw = &self.state.texture_window;
                (tw.texture_window_mask_x as u32)
                    | (tw.texture_window_mask_y as u32) << 5
                    | (tw.texture_window_offset_x as u32) << 10
                    | (tw.texture_window_offset_y as u32) << 15
            }
            0x3 => {
                let da = &self.state.drawing_area;
                (da.x1 as u32) | (da.y1 as u32) << 10
            }
            0x4 => {
                let da = &self.state.drawing_area;
                (da.x2 as u32) | (da.y2 as u32) << 10
            }
            0x5 => {
                let off = &self.state.drawing_offset;
                (off.x as u32 & 0x7FF) | ((off.y as u32 & 0x7FF) << 11)
            }
            0x7 => 2, // GPU type 2
            0x8 => 0, // unknown/unused, usually 0
            _ => 0,
        };

        self.gp0_read_mode = Some(Gp0ReadMode::GpuInfo(sub_index));
        self.gpuread_last_value = value;
    }
}

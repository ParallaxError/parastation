/*
 * @file /parastation-core/src/gpu/gpu_state.rs
 * @brief
 * State encapsulation of the GPU as structs, defined at
 * https://problemkaputt.de/psx-spx.htm#gpustatusregister.
 *
 * The GPU state is encapsulated in the GpuState struct, which holds multiple xState structs
 * for the different GPU registers.
 *
 * -----
 */

/// Draw mode state set by GP0(E1h)
#[derive(Debug, Clone, Default)]
pub struct DrawMode {
    pub texture_base_x: u8,
    pub texture_base_y: bool,
    pub semi_transparency: u8,
    pub texture_page_colours: u8,
    pub dither: bool,
    pub draw_to_display: bool,
    pub texture_disable: bool,
    pub textured_rectangle_flip_x: bool,
    pub textured_rectangle_flip_y: bool,
}

impl DrawMode {
    pub fn from_gp0_command(command: u32) -> Self {
        /*
        0-3   Texture page X Base   (N*64) (ie. in 64-halfword steps)    ;GPUSTAT.0-3
        4     Texture page Y Base   (N*256) (ie. 0 or 256)               ;GPUSTAT.4
        5-6   Semi Transparency     (0=B/2+F/2, 1=B+F, 2=B-F, 3=B+F/4)   ;GPUSTAT.5-6
        7-8   Texture page colors   (0=4bit, 1=8bit, 2=15bit, 3=Reserved);GPUSTAT.7-8
        9     Dither 24bit to 15bit (0=Off/strip LSBs, 1=Dither Enabled) ;GPUSTAT.9
        10    Drawing to display area (0=Prohibited, 1=Allowed)          ;GPUSTAT.10
        11    Texture Disable (0=Normal, 1=Disable if GP1(09h).Bit0=1)   ;GPUSTAT.15
        12    Textured Rectangle X-Flip
        13    Textured Rectangle Y-Flip
        14-23 Not used (should be 0)
        24-31 Command  (E1h)
        */
        Self {
            texture_base_x: ((command >> 0) & 0xF) as u8,
            texture_base_y: ((command >> 4) & 0x1) != 0,
            semi_transparency: ((command >> 5) & 0x3) as u8,
            texture_page_colours: ((command >> 7) & 0x3) as u8,
            dither: ((command >> 9) & 0x1) != 0,
            draw_to_display: ((command >> 10) & 0x1) != 0,
            texture_disable: ((command >> 11) & 0x1) != 0,
            textured_rectangle_flip_x: ((command >> 12) & 0x1) != 0,
            textured_rectangle_flip_y: ((command >> 13) & 0x1) != 0,
        }
    }
}

/// Texture window setting set by GP0(E2h)
#[derive(Debug, Clone, Default)]
pub struct TextureWindow {
    pub texture_window_mask_x: u8,
    pub texture_window_mask_y: u8,
    pub texture_window_offset_x: u8,
    pub texture_window_offset_y: u8,
}

impl TextureWindow {
    pub fn from_gp0_command(command: u32) -> Self {
        /*
        0-4    Texture window Mask X   (in 8 pixel steps)
        5-9    Texture window Mask Y   (in 8 pixel steps)
        10-14  Texture window Offset X (in 8 pixel steps)
        15-19  Texture window Offset Y (in 8 pixel steps)
        20-23  Not used (zero)
        24-31  Command  (E2h)
        */
        Self {
            texture_window_mask_x: ((command >> 0) & 0x1F) as u8,
            texture_window_mask_y: ((command >> 5) & 0x1F) as u8,
            texture_window_offset_x: ((command >> 10) & 0x1F) as u8,
            texture_window_offset_y: ((command >> 15) & 0x1F) as u8,
        }
    }
}

/// Drawing area corners set by GP0(E3h) and GP0(E4h)
#[derive(Debug, Clone, Default)]
pub struct DrawingArea {
    pub x1: u16,
    pub y1: u16,
    pub x2: u16,
    pub y2: u16,
}

impl DrawingArea {
    /*
    0-9    X-coordinate (0..1023)
    10-18  Y-coordinate (0..511) on retail 1MB VRAM consoles
    24-31  Command (Exh)
    */
    pub fn set_top_left(&mut self, command: u32) {
        self.x1 = (command & 0x3FF) as u16;
        self.y1 = ((command >> 10) & 0x1FF) as u16;
    }

    pub fn set_bottom_right(&mut self, command: u32) {
        self.x2 = (command & 0x3FF) as u16;
        self.y2 = ((command >> 10) & 0x1FF) as u16;
    }
}

// Helper for parsing drawing offset
fn sign_extend(value: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((value << shift) as i32) >> shift
}

/// Drawing offset set by GP0(E5h)
#[derive(Debug, Clone, Default)]
pub struct DrawingOffset {
    pub x: i16,
    pub y: i16,
}

impl DrawingOffset {
    pub fn from_gp0_command(command: u32) -> Self {
        /*
        0-10   X-offset (-1024..+1023)
        11-21  Y-offset (-1024..+1023)
        22-23  Not used (zero)
        24-31  Command  (E5h)
        */
        Self {
            x: sign_extend(command, 11) as i16,
            y: sign_extend(command >> 11, 11) as i16,
        }
    }
}

/// Mask bit setting set by GP0(E6h)
#[derive(Debug, Clone, Default)]
pub struct Mask {
    pub set_mask_while_drawing: bool,
    pub check_mask_before_draw: bool,
}

impl Mask {
    pub fn from_gp0_command(command: u32) -> Self {
        /*
        0     Set mask while drawing (0=TextureBit15, 1=ForceBit15=1)   ;GPUSTAT.11
        1     Check mask before draw (0=Draw Always, 1=Draw if Bit15=0) ;GPUSTAT.12
        2-23  Not used (zero)
        24-31 Command  (E6h)
        */
        Self {
            set_mask_while_drawing: (command & 0x1) != 0,
            check_mask_before_draw: ((command >> 1) & 0x1) != 0,
        }
    }
}

/// Display state set by GP1 commands
#[derive(Debug, Clone)]
pub struct DisplayState {
    pub display_enable: bool,
    pub dma_direction: u8,
    pub display_start_x: u16,
    pub display_start_y: u16,
    pub horizontal_range_x1: u16,
    pub horizontal_range_x2: u16,
    pub vertical_range_y1: u16,
    pub vertical_range_y2: u16,
    pub horizontal_resolution_1: u8,
    pub vertical_resolution: bool,
    pub video_mode: bool,
    pub display_colour_depth: bool,
    pub vertical_interlace: bool,
    pub horizontal_resolution_2: bool,
    pub reverseflag: bool,
    pub texture_disable_allowed: bool, // NOT reset by GP1(00h)
}

impl Default for DisplayState {
    fn default() -> Self {
        // Reset values per GP1(00h) spec
        Self {
            display_enable: false,
            dma_direction: 0,
            display_start_x: 0,
            display_start_y: 0,
            horizontal_range_x1: 0x200,
            horizontal_range_x2: 0x200 + 256 * 10,
            vertical_range_y1: 0x010,
            vertical_range_y2: 0x010 + 240,
            horizontal_resolution_1: 0,
            vertical_resolution: false,
            video_mode: false,
            display_colour_depth: false,
            vertical_interlace: false,
            horizontal_resolution_2: false,
            reverseflag: false,
            texture_disable_allowed: false,
        }
    }
}

// Passed to the GPU backend to describe how the display should be presented
pub struct DisplayOutput {
    pub enabled: bool,
    pub vram_x: u16,
    pub vram_y: u16,
    pub width_px: u16,
    pub height_px: u16,
    pub colour_depth: bool,        // false = 15bpp, true = 24bpp
    pub display_aspect_ratio: f32, // width:height ratio the output should be presented at
}

impl DisplayState {
    // NTSC analog TV horizontal sampling rate for 1:1 aspect ratio
    // https://github.com/libretro/beetle-psx-libretro/issues/510
    const NTSC_1_1_PAR_240P_HZ: f64 = 135_000_000.0 / 22.0;

    // https://psx-spx.consoledev.net/graphicsprocessingunitgpu/
    const fn gpu_dotclock_hz(width_px: u16) -> f64 {
        const BASE: f64 = 44_100.0 * 0x300 as f64 * 11.0 / 7.0;
        let divider: f64 = match width_px {
            256 => 10.0,
            320 => 8.0,
            368 => 7.0,
            512 => 5.0,
            640 => 4.0,
            _ => 8.0, // fall back to 320's divider
        };
        BASE / divider
    }

    /// Get the pixel aspect ratio at 240p for a given horizontal resolution, derived from how fast the GPU dotclock
    /// is compared to the NTSC 1:1 aspect ratio dotclock
    const fn pixel_aspect_ratio_240p(width_px: u16) -> f64 {
        Self::NTSC_1_1_PAR_240P_HZ / Self::gpu_dotclock_hz(width_px)
    }

    /// Get the display output parameters based on the current display state
    pub fn derive_output(&self) -> DisplayOutput {
        let width_px: u16 = if self.horizontal_resolution_2 {
            368
        } else {
            match self.horizontal_resolution_1 & 0x3 {
                0 => 256,
                1 => 320,
                2 => 512,
                3 => 640,
                _ => unreachable!(),
            }
        };

        let interlaced_480 = self.vertical_resolution && self.vertical_interlace;
        let height_px: u16 = if interlaced_480 { 480 } else { 240 };

        let mut par = Self::pixel_aspect_ratio_240p(width_px);
        if interlaced_480 {
            par *= 2.0;
        }

        // Display aspect ratio = (width_px * PAR) / height_px
        let display_aspect_ratio = (width_px as f64 * par / height_px as f64) as f32;

        DisplayOutput {
            enabled: !self.display_enable, // (0=On, 1=Off)
            vram_x: self.display_start_x,
            vram_y: self.display_start_y,
            width_px,
            height_px,
            colour_depth: self.display_colour_depth,
            display_aspect_ratio,
        }
    }
}

/// GPU state encapsulation struct. Holds all stateful registers on the PS1 GPU.
#[derive(Debug, Clone, Default)]
pub struct GpuState {
    pub irq: bool,
    pub draw_mode: DrawMode,
    pub texture_window: TextureWindow,
    pub drawing_area: DrawingArea,
    pub drawing_offset: DrawingOffset,
    pub mask: Mask,
    pub display_state: DisplayState,
}

/// Reset GPU state per GP1(00h), preserving texture_disable_allowed per spec
impl GpuState {
    pub fn reset(&mut self) {
        let texture_disable_allowed = self.display_state.texture_disable_allowed;
        *self = Self::default();
        self.display_state.texture_disable_allowed = texture_disable_allowed;
    }
}

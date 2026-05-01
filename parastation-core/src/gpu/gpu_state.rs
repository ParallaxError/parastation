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
            texture_base_x:             ((command >> 0) & 0xF) as u8,
            texture_base_y:             ((command >> 4) & 0x1) != 0,
            semi_transparency:          ((command >> 5) & 0x3) as u8,
            texture_page_colours:       ((command >> 7) & 0x3) as u8,
            dither:                     ((command >> 9) & 0x1) != 0,
            draw_to_display:            ((command >> 10) & 0x1) != 0,
            texture_disable:            ((command >> 11) & 0x1) != 0,
            textured_rectangle_flip_x:  ((command >> 12) & 0x1) != 0,
            textured_rectangle_flip_y:  ((command >> 13) & 0x1) != 0,
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
            texture_window_mask_x:    ((command >> 0)  & 0x1F) as u8,
            texture_window_mask_y:    ((command >> 5)  & 0x1F) as u8,
            texture_window_offset_x:  ((command >> 10) & 0x1F) as u8,
            texture_window_offset_y:  ((command >> 15) & 0x1F) as u8,
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
            x: ((command as i32) << 22 >> 22) as i16,
            y: (((command >> 11) as i32) << 21 >> 21) as i16,
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
            display_enable:          false,
            dma_direction:           0,
            display_start_x:         0,
            display_start_y:         0,
            horizontal_range_x1:     0x200,
            horizontal_range_x2:     0x200 + 256 * 10,
            vertical_range_y1:       0x010,
            vertical_range_y2:       0x010 + 240,
            horizontal_resolution_1: 0,
            vertical_resolution:     false,
            video_mode:              false,
            display_colour_depth:    false,
            vertical_interlace:      false,
            horizontal_resolution_2: false,
            reverseflag:             false,
            texture_disable_allowed: false,
        }
    }
}

/// GPU state encapsulation struct. Holds all stateful registers on the PS1 GPU.
#[derive(Debug, Clone, Default)]
pub struct GpuState {
    pub irq:             bool,
    pub draw_mode:       DrawMode,
    pub texture_window:  TextureWindow,
    pub drawing_area:    DrawingArea,
    pub drawing_offset:  DrawingOffset,
    pub mask:            Mask,
    pub display_state:   DisplayState,
}

/// Reset GPU state per GP1(00h), preserving texture_disable_allowed per spec
impl GpuState {
    pub fn reset(&mut self) {
        let texture_disable_allowed = self.display_state.texture_disable_allowed;
        *self = Self::default();
        self.display_state.texture_disable_allowed = texture_disable_allowed;
    }
}
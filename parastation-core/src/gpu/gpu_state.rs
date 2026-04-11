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

// Command GP0 0xE1: Draw mode setting
pub struct DrawMode {
    pub texture_base_x: u8, // N * 64 (in 64 halfword steps)
    pub texture_base_y: bool, // N * 256 (0 or 256, just a bit)
    pub semi_transparency: u8, // 0=B/2+F/2, 1=B+F, 2=B-F, 3=B+F/4
    pub texture_page_colours: u8, // (0=4bit, 1=8bit, 2=15bit, 3=Reserved)
    pub dither: bool, // 0=Off, 1=On
    pub draw_to_display: bool, // 0=Prohibited, 1=Allowed
    pub texture_disable: bool, // 0=Off, 1=On
    pub textured_rectangle_flip_x: bool,
    pub textured_rectangle_flip_y: bool,
}

impl DrawMode {
    fn new() -> Self {
        Self {
            texture_base_x: 0,
            texture_base_y: false,
            semi_transparency: 0,
            texture_page_colours: 0,
            dither: false,
            draw_to_display: false,
            texture_disable: false,
            textured_rectangle_flip_x: false,
            textured_rectangle_flip_y: false,
        }
    }
}

// Command GP0 0xE2: Texture window setting
pub struct TextureWindow {
    pub texture_window_mask_x: u8, // N * 8 (in 8 pixel steps)
    pub texture_window_mask_y: u8, // N * 8 (in 8 pixel steps)
    pub texture_window_offset_x: u8, // N * 8 (in 8 pixel steps)
    pub texture_window_offset_y: u8, // N * 8 (in 8 pixel steps)
}

impl TextureWindow {
    fn new() -> Self {
        Self {
            texture_window_mask_x: 0,
            texture_window_mask_y: 0,
            texture_window_offset_x: 0,
            texture_window_offset_y: 0,
        }
    }
}

// Command GP0 0xE3/0xE4: Drawing area setting
pub struct DrawingArea {
    pub x1: u16,
    pub y1: u16,
    pub x2: u16,
    pub y2: u16
}

impl DrawingArea {
    fn new() -> Self {
        Self {
            x1: 0,
            y1: 0,
            x2: 0,
            y2: 0,
        }
    }
}

// Command GP0 0xE5: Drawing offset setting
pub struct DrawingOffset {
    pub drawing_offset_x: i16,
    pub drawing_offset_y: i16,
}

impl DrawingOffset {
    fn new() -> Self {
        Self {
            drawing_offset_x: 0,
            drawing_offset_y: 0,
        }
    }
}

// Command GP0 0xE6: Mask setting
pub struct Mask {
    pub set_mask_while_drawing: bool,
    pub check_mask_before_draw: bool,
}

impl Mask {
    fn new() -> Self {
        Self {
            set_mask_while_drawing: false,
            check_mask_before_draw: false,
        }
    }
}

// GP1 state (Display settings)
pub struct DisplayState {
    pub display_enable: bool, // (0=Off, 1=On): GP1 0x03h
    pub dma_direction: u8, // (0=Off, 1=FIFO, 2=CPUtoGP0, 3=GPUREADtoCPU): GP1 0x04h
    pub display_start_x: u16, // (0-1023), GP1 0x05h
    pub display_start_y: u16, // (0-511), GP1 0x05h
    pub horizontal_range_x1: u16, // (12bit), GP1 0x06h
    pub horizontal_range_x2: u16, // (12bit), GP1 0x06h
    pub vertical_range_y1: u16, // GP1 0x07h
    pub vertical_range_y2: u16, // GP1 0x07h

    // Display mode (GP1 0x08h)
    pub horizontal_resolution_1: u8, // (0=256, 1=320, 2=512, 3=640)
    pub vertical_resolution: bool, // (0=240, 1=480 when Bit5=1?)
    pub video_mode: bool, // (0=NTSC, 1=PAL)
    pub display_colour_depth: bool, // (0=15bit, 1=24bit)
    pub vertical_interlace: bool,
    pub horizontal_resolution_2: bool, // (0=256/320/512/640, 1=368)
    pub reverseflag: bool, // (0=Normal, 1=Distorted?)

    pub texture_disable_allowed: bool, // (0=Off, 1=On): GP1 0x09h
}

impl DisplayState {
    fn new() -> Self {
        Self {
            display_enable: false,
            dma_direction: 0,
            display_start_x: 0,
            display_start_y: 0,
            horizontal_range_x1: 0,
            horizontal_range_x2: 0,
            vertical_range_y1: 0,
            vertical_range_y2: 0,
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

/// GPU state encapsulation struct. Holds all stateful registers on the PS1 GPU
pub struct GpuState {
    pub irq: bool, // GPU interrupt request flag (GP0 0x1F/GP1 0x02)

    pub draw_mode: DrawMode,
    pub texture_window: TextureWindow,
    pub drawing_area: DrawingArea,
    pub drawing_offset: DrawingOffset,
    pub mask: Mask,
    pub display_state: DisplayState,
}

impl GpuState {
    pub fn new() -> Self {
        Self {
            irq: false,

            draw_mode: DrawMode::new(),
            texture_window: TextureWindow::new(),
            drawing_area: DrawingArea::new(),
            drawing_offset: DrawingOffset::new(),
            mask: Mask::new(),
            display_state: DisplayState::new(),
        }
    }
}
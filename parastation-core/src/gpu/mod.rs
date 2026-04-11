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
use gpu_state::GpuState;
mod rendering_parameters;
pub mod backend;
pub use backend::GpuBackend;

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
            state: GpuState::new(),
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
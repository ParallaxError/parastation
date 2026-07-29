/*
 * @file /parastation-frontend/src/runner.rs
 * @brief
 * Handles the window and GL context creation and main loop for the ParaStation frontend.
 * Attempts to drive the emulation at a fixed framerate using both sleep and busy-waiting (spinning) to avoid dropping
 * frames, and prints a summary of the average framerate and frame time at the end of execution
 *
 * -----
 */

// Imports
use std::time::{Duration, Instant};

use parastation_core::bios::Bios;
use parastation_core::{DiscSource, Interpreter, Ps1};

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::{Display, GetGlDisplay};
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowBuilder};

use crate::gl_texture_barrier::RawGlExt;
use crate::keyboard_input_provider::{DummyInputProvider, KeyboardInputProvider, KeyboardState};
use crate::opengl_backend::OpenGlBackend;
use crate::spu_backend::CpalSpuBackend;

// File acquisition methods for the CD-ROM drive, using std::fs::File and std::io::Read + Seek to read from the disc
// image files
struct NativeFile(std::fs::File);

impl DiscSource for NativeFile {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> usize {
        use std::io::{Read, Seek, SeekFrom};
        if self.0.seek(SeekFrom::Start(offset)).is_err() {
            return 0;
        }
        self.0.read(buf).unwrap_or(0)
    }

    fn len(&self) -> u64 {
        self.0.metadata().map(|m| m.len()).unwrap_or(0)
    }
}

struct NativeLogger;

impl parastation_core::logging::Logger for NativeLogger {
    fn log(&self, message: &str) {
        println!("{message}");
    }
    fn elog(&self, message: &str) {
        eprintln!("{message}");
    }
    fn tty_putchar(&self, ch: char) {
        print!("{ch}");
        use std::io::Write;
        std::io::stdout().flush().ok();
    }
}

// Target framerate and parameters
const TARGET_FPS: f64 = 60.0;
const CYCLES_PER_FRAME: u64 = 564_480; // 33,868,800 Hz / 60
const SPIN_THRESHOLD: Duration = Duration::from_micros(1500); // 1.5ms, threshold for busy-waiting instead of sleeping

/// Owns the window, GL context and the PS1 core to drive the main emulation loop of the frontend at a steady framerate
pub struct Runner {
    ps1: Ps1<Interpreter>,
    keyboard_state: KeyboardState,

    _window: Window, // Kept as member just to keep the window alive
    gl_context: PossiblyCurrentContext,
    gl_surface: Surface<WindowSurface>,

    frame_duration: Duration,
    next_frame_time: Instant,

    session_start: Instant,
    total_cycles_run: u64,
    total_frames_displayed: u64,

    uncapped: bool, // Whether to run the emulation loop uncapped (as fast as possible) or capped to TARGET_FPS
}

impl Runner {
    fn set_high_timer_resolution() {
        #[cfg(target_os = "windows")]
        {
            use winapi::um::timeapi::timeBeginPeriod;
            unsafe { timeBeginPeriod(1) }; // Set timer resolution to 1ms
        }
    }

    pub fn new(event_loop: &EventLoop<()>, bios: Bios) -> Self {
        Self::set_high_timer_resolution();

        let window_builder = WindowBuilder::new()
            .with_title("ParaStation")
            .with_inner_size(LogicalSize::new(1024u32, 512u32));

        let template = ConfigTemplateBuilder::new().with_alpha_size(8);
        let display_builder = DisplayBuilder::new().with_window_builder(Some(window_builder));

        let (window, gl_config) = display_builder
            .build(event_loop, template, |configs| {
                configs
                    .reduce(|a, b| {
                        if a.num_samples() > b.num_samples() {
                            a
                        } else {
                            b
                        }
                    })
                    .unwrap()
            })
            .unwrap();

        let window = window.unwrap();
        let raw_window_handle = window.raw_window_handle();

        let context_attrs = ContextAttributesBuilder::new().build(Some(raw_window_handle));

        let display: Display = gl_config.display();
        let context = unsafe { display.create_context(&gl_config, &context_attrs).unwrap() };

        let size = window.inner_size();
        let surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_window_handle,
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        );

        let gl_surface = unsafe {
            display
                .create_window_surface(&gl_config, &surface_attrs)
                .unwrap()
        };

        let gl_context = context.make_current(&gl_surface).unwrap();

        let gl = unsafe {
            glow::Context::from_loader_function(|s| {
                display.get_proc_address(&std::ffi::CString::new(s).unwrap()) as *const _
            })
        };

        let raw_ext = RawGlExt::load(|s| {
            display.get_proc_address(&std::ffi::CString::new(s.to_str().unwrap()).unwrap())
                as *const _
        });

        let keyboard_state = KeyboardState::new();
        let keyboard_input_provider = KeyboardInputProvider::new(keyboard_state.clone());

        let gpu_backend = Box::new(OpenGlBackend::new(gl, raw_ext));
        let spu_backend = Box::new(CpalSpuBackend::new());

        let ps1 = Ps1::new(
            bios,
            Interpreter::new(),
            gpu_backend,
            spu_backend,
            Box::new(keyboard_input_provider),
            Box::new(DummyInputProvider),
        );

        parastation_core::logging::set_logger(Box::new(NativeLogger));

        let frame_duration = Duration::from_secs_f64(1.0 / TARGET_FPS);
        let next_frame_time = Instant::now() + frame_duration;

        Self {
            ps1,
            keyboard_state,
            _window: window,
            gl_context,
            gl_surface,
            frame_duration,
            next_frame_time,
            session_start: Instant::now(),
            total_cycles_run: 0,
            total_frames_displayed: 0,
            uncapped: false,
        }
    }
}

// Main loop and event handling
impl Runner {
    pub fn run(mut self, event_loop: EventLoop<()>) {
        event_loop
            .run(move |event, elwt| match event {
                Event::AboutToWait => {
                    self.tick_frame();
                    elwt.set_control_flow(ControlFlow::Poll);
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    self.print_session_summary();
                    elwt.exit();
                }
                Event::WindowEvent {
                    event:
                        WindowEvent::KeyboardInput {
                            event: key_event, ..
                        },
                    ..
                } => {
                    if let PhysicalKey::Code(key_code) = key_event.physical_key {
                        match key_event.state {
                            ElementState::Pressed => self.keyboard_state.key_pressed(key_code),
                            ElementState::Released => self.keyboard_state.key_released(key_code),
                        }
                    }
                }
                _ => (),
            })
            .unwrap();
    }

    /// Runs one PS1 frame if it's time, and sleeps/spins until the next frame time
    fn tick_frame(&mut self) {
        if self.uncapped {
            self.ps1.run(CYCLES_PER_FRAME);
            self.total_cycles_run += CYCLES_PER_FRAME;

            self.ps1.display();
            self.total_frames_displayed += 1;

            self.gl_surface.swap_buffers(&self.gl_context).unwrap();
            return; // no wait at all - run flat out
        }

        let now = Instant::now();

        if now >= self.next_frame_time {
            self.ps1.run(CYCLES_PER_FRAME);
            self.total_cycles_run += CYCLES_PER_FRAME;

            self.ps1.display();
            self.total_frames_displayed += 1;

            self.gl_surface.swap_buffers(&self.gl_context).unwrap();

            self.next_frame_time += self.frame_duration;
            if self.next_frame_time < now {
                self.next_frame_time = now + self.frame_duration;
            }
        }

        // Sleep for the majority of the remaining time (cheap for the CPU), but then busy wait for the remaining
        // duration so that waking up is faster than waiting for Windows, so we can avoid some "slippage" in the FPS
        let remaining = self
            .next_frame_time
            .saturating_duration_since(Instant::now());
        if remaining > SPIN_THRESHOLD {
            std::thread::sleep(remaining - SPIN_THRESHOLD);
        }
        while Instant::now() < self.next_frame_time {
            std::hint::spin_loop();
        }
    }

    fn print_session_summary(&self) {
        let elapsed = self.session_start.elapsed();
        let expected_cycles = (elapsed.as_secs_f64() * 33_868_800.0) as u64;
        let expected_frames = (elapsed.as_secs_f64() * TARGET_FPS) as u64;

        println!("\n=== Session Summary ===");
        println!("Wall-clock elapsed:     {:.3}s", elapsed.as_secs_f64());
        println!("Total cycles run:       {}", self.total_cycles_run);
        println!("Expected cycles:        {}", expected_cycles);
        println!(
            "Cycle speed ratio:      {:.3}x real PS1 speed",
            self.total_cycles_run as f64 / expected_cycles.max(1) as f64
        );
        println!("Total frames displayed: {}", self.total_frames_displayed);
        println!("Expected frames:        {}", expected_frames);
    }
}

// Exposed methods
impl Runner {
    pub fn set_uncapped(&mut self, uncapped: bool) {
        self.uncapped = uncapped;
    }
}

// PS1 exposed methods
impl Runner {
    pub fn insert_cdrom_disc(&mut self, cue_path: &str) {
        let cue_content = std::fs::read_to_string(cue_path).expect("Failed to read CUE file");
        let cue_directory = std::path::Path::new(cue_path)
            .parent()
            .unwrap()
            .to_path_buf();

        self.ps1.insert_cdrom_disc(&cue_content, |filename| {
            let file_path = cue_directory.join(filename);
            let file = std::fs::File::open(file_path).expect("Failed to open disc image file");
            Box::new(NativeFile(file))
        });
    }

    pub fn run_until_pc_and_load_exe(&mut self, target_pc: u32, exe_path: &str) {
        self.ps1.run_until_pc(target_pc);

        // Load exe data from file and load it into PS1 memory
        let exe_data = std::fs::read(exe_path).unwrap_or_else(|e| {
            eprintln!("Failed to load exe file: {e}");
            std::process::exit(1);
        });

        self.ps1.load_exe(&exe_data);
    }
}

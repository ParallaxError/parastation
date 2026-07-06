mod dummy_gpu_backend;
mod opengl_backend;
use opengl_backend::OpenGlBackend;
mod keyboard_input_provider;
use keyboard_input_provider::*;

use std::env;
use std::time::{Duration, Instant};

use parastation_core::bios::Bios;
use parastation_core::sio0::InputProvider;
use parastation_core::{Interpreter, Ps1};

use glutin::config::ConfigTemplateBuilder;
use glutin::context::ContextAttributesBuilder;
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::WindowBuilder;

pub struct DummyInputProvider;
impl InputProvider for DummyInputProvider {
    fn get_joypad_state(&self) -> u16 {
        0xFFFF // All buttons released
    }
}

fn main() {
    unsafe {
        env::set_var("RUST_BACKTRACE", "1");
    }
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: ps1-frontend <bios.bin>");
        std::process::exit(1);
    }

    let bios = Bios::load_from_file(&args[1]).unwrap_or_else(|e| {
        eprintln!("Failed to load BIOS: {e}");
        std::process::exit(1);
    });

    // Build wiindow and GL display
    let event_loop = EventLoop::new().unwrap();
    let window_builder = WindowBuilder::new()
        .with_title("ParaStation")
        .with_inner_size(LogicalSize::new(1024u32, 512u32));

    let template = ConfigTemplateBuilder::new().with_alpha_size(8);
    let display_builder = DisplayBuilder::new().with_window_builder(Some(window_builder));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |configs| {
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

    let display = gl_config.display();
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

    // Create glow context from glutin
    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            display.get_proc_address(&std::ffi::CString::new(s).unwrap()) as *const _
        })
    };

    // Create the controller for the keyboard input provider
    let keyboard_state = KeyboardState::new();
    let keyboard_input_provider = KeyboardInputProvider::new(keyboard_state.clone());

    // Finally, create the backend
    let backend = Box::new(OpenGlBackend::new(gl));
    let mut ps1 = Ps1::new(
        bios,
        Interpreter::new(),
        backend,
        Box::new(keyboard_input_provider),
        Box::new(DummyInputProvider),
    );

    // Insert disk
    ps1.insert_cdrom_disc(r"games\Ridge Racer\Ridge Racer.cue");
    // ps1.insert_cdrom_disc("tests\\nolibgs_hello_worlds\\hello_cd\\hello_cd.cue");

    // Run some bios
    ps1.run_until_pc(0x80030000);

    // Load test exe
    let exe_data = std::fs::read("tests/test-all.exe").unwrap_or_else(|e| {
        eprintln!("Failed to load psxtest_cpu.exe: {e}");
        std::process::exit(1);
    });
    ps1.load_exe(&exe_data);

    // Only call display at a set FPS
    let frame_duration = Duration::from_secs_f64(1.0 / 60.0);
    let mut last_display = Instant::now();

    // Run PS1
    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);

            match event {
                Event::AboutToWait => {
                    ps1.run(33000);
                    if last_display.elapsed() >= frame_duration {
                        ps1.display();
                        gl_surface.swap_buffers(&gl_context).unwrap();
                        last_display = Instant::now();
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => elwt.exit(),
                Event::WindowEvent {
                    event:
                        WindowEvent::KeyboardInput {
                            event: key_event, ..
                        },
                    ..
                } => {
                    if let PhysicalKey::Code(key_code) = key_event.physical_key {
                        match key_event.state {
                            ElementState::Pressed => keyboard_state.key_pressed(key_code),
                            ElementState::Released => keyboard_state.key_released(key_code),
                        }
                    }
                }
                _ => (),
            }
        })
        .unwrap();
}

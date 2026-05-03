mod dummy_gpu_backend; 
mod opengl_backend;
use opengl_backend::OpenGlBackend;

use parastation_core::{Interpreter, Ps1};
use parastation_core::bios::Bios;
use std::env;

use glutin::config::ConfigTemplateBuilder;
use glutin::context::ContextAttributesBuilder;
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

fn main() {
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
                    if a.num_samples() > b.num_samples() { a } else { b }
                })
                .unwrap()
        })
        .unwrap();

    let window = window.unwrap();
    let raw_window_handle = window.raw_window_handle();

    let context_attrs = ContextAttributesBuilder::new().build(Some(raw_window_handle));

    let display = gl_config.display();
    let context = unsafe {
        display.create_context(&gl_config, &context_attrs).unwrap()
    };

    let size = window.inner_size();
    let surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new()
        .build(raw_window_handle, size.width.try_into().unwrap(), size.height.try_into().unwrap());

    let gl_surface = unsafe {
        display.create_window_surface(&gl_config, &surface_attrs).unwrap()
    };

    let gl_context = context.make_current(&gl_surface).unwrap();

    // Create glow context from glutin
    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            display.get_proc_address(&std::ffi::CString::new(s).unwrap()) as *const _
        })
    };

    // Finally, create the backend
    let backend = Box::new(OpenGlBackend::new(gl));
    let mut ps1 = Ps1::new(bios, Interpreter::new(), backend);

    // Run some bios
    ps1.run_until_pc(0x80030000);
    
    // Load test exe
    let exe_data = std::fs::read("tests/psxtest_cpu.exe").unwrap_or_else(|e| {
        eprintln!("Failed to load psxtest_cpu.exe: {e}");
        std::process::exit(1);
    });
    // ps1.load_exe(&exe_data);

    // Run tesI
    println!("Starting amidog test!");
    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);

            match event {
                Event::AboutToWait => {
                    ps1.run(33000);
                    ps1.display();
                    gl_surface.swap_buffers(&gl_context).unwrap();
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => elwt.exit(),
                _ => (),
            }
        })
        .unwrap();
}

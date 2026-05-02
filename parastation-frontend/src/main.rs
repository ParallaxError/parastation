mod dummy_gpu_backend; 
use dummy_gpu_backend::DummyGpuBackend;
mod software_backend;
use software_backend::SoftwareGpuBackend;

use parastation_core::{GpuBackend, Interpreter, Ps1};
use parastation_core::bios::Bios;
use std::env;

use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};

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

    let event_loop = EventLoop::new();
    let backend = Box::new(SoftwareGpuBackend::new(&event_loop));
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
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::MainEventsCleared => {
                ps1.run(33000);
                ps1.display();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => (),
        }
    });
}

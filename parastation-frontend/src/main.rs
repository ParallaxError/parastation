mod dummy_gpu_backend; 
use dummy_gpu_backend::DummyGpuBackend;

use parastation_core::{Ps1, Interpreter};
use parastation_core::bios::Bios;
use std::env;

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

    let backend = Box::new(DummyGpuBackend::new());
    let mut ps1 = Ps1::new(bios, Interpreter::new(), backend);

    // Run some bios
    ps1.run_until_pc(0x80030000);
    
    // Load test exe
    let exe_data = std::fs::read("tests/psxtest_cpu.exe").unwrap_or_else(|e| {
        eprintln!("Failed to load test.exe: {e}");
        std::process::exit(1);
    });
    ps1.load_exe(&exe_data);

    // Run test
    loop {
        ps1.run(1000);
    }
}
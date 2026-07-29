mod gl_texture_barrier;
mod keyboard_input_provider;
mod opengl_backend;
mod runner;
mod spu_backend;

use parastation_core::bios::Bios;
use runner::Runner;
use std::env;
use winit::event_loop::EventLoop;

fn main() {
    unsafe {
        env::set_var("RUST_BACKTRACE", "1");
    }

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: ps1-frontend <bios.bin>");
        std::process::exit(1);
    }

    // Load bios data into a Box<[u8]> and create a Bios instance
    let bios_data = std::fs::read(&args[1]).expect("Failed to read BIOS file");
    let bios = Bios::new(bios_data.into_boxed_slice());

    let event_loop = EventLoop::new().unwrap();
    let mut runner = Runner::new(&event_loop, bios);
    runner.set_uncapped(true);

    runner.insert_cdrom_disc(r"games/Crash/Crash Bandicoot.cue");
    // runner.run_until_pc_and_load_exe(0x80030000, r"tests/mdec/frame/frame-24bit-dma.exe");
    runner.run(event_loop);
}

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

    let bios = Bios::load_from_file(&args[1]).unwrap_or_else(|e| {
        eprintln!("Failed to load BIOS: {e}");
        std::process::exit(1);
    });

    let event_loop = EventLoop::new().unwrap();
    let mut runner = Runner::new(&event_loop, bios);
    // runner.set_uncapped(true);

    runner.insert_cdrom_disc(r"games\MGS1\Metal Gear Solid (Disc 1).cue");
    runner.run(event_loop);
}

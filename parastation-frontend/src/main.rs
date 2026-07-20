mod keyboard_input_provider;
mod opengl_backend;
mod runner;
mod spu_backend;

use std::env;
use parastation_core::bios::Bios;
use winit::event_loop::EventLoop;
use runner::Runner;

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

    runner.insert_cdrom_disc(r"games\Crash Bandicoot [NTSC-U] [SCUS-94900]\Crash Bandicoot (USA).cue");
    runner.run(event_loop);
}

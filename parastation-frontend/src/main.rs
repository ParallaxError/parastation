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

    let mut ps1 = Ps1::new(bios, Interpreter::new());

    loop {
        ps1.run(1);
    }
}
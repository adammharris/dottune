//! A tune's event list (SPEC §6), from a file or stdin.
//!
//!     cargo run -p tune --bin events -- examples/ode-to-joy.tune

use std::io::Read;

fn main() {
    let source = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("{path}: {e}");
            std::process::exit(1)
        }),
        None => {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .expect("stdin is UTF-8");
            s
        }
    };
    match tune::compile(&source) {
        Ok(song) => println!("{}", tune::format_song(&song)),
        Err(errors) => {
            for e in &errors.0 {
                eprintln!("{e}");
            }
            std::process::exit(1);
        }
    }
}

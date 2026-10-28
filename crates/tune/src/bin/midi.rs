//! A tune as a Standard MIDI File.
//!
//!     cargo run -p tune --bin midi -- examples/ode-to-joy.tune ode.mid

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: midi <in.tune> <out.mid>");
        std::process::exit(2);
    };
    let source = std::fs::read_to_string(input).unwrap_or_else(|e| {
        eprintln!("{input}: {e}");
        std::process::exit(1)
    });
    let song = tune::compile(&source).unwrap_or_else(|errors| {
        for e in &errors.0 {
            eprintln!("{e}");
        }
        std::process::exit(1)
    });
    std::fs::write(output, tune::midi::write_midi(&song)).unwrap_or_else(|e| {
        eprintln!("{output}: {e}");
        std::process::exit(1)
    });
    println!("{} notes → {output}", song.events.len());
}

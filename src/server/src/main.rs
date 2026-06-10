use omenchatd::{CliCommand, Omenchatd};

fn main() {
    let command = CliCommand::parse(std::env::args().skip(1));
    if let Err(error) = Omenchatd.run(command) {
        eprintln!("omenchatd: {error}");
        std::process::exit(1);
    }
}

//! GUI-16, GUI-17
//! `sim-gui [+cmd] [dir | file]`: sim in a window. The arguments are sim's own.
use std::process::ExitCode;

/// What `sim-gui --help` prints (`{version}` is the Cargo version).
const HELP: &str = include_str!("../help.txt");

fn main() -> ExitCode {
    // (`env::args` panics on an argument that is not UTF-8; every path in sim is text)
    let args: Option<Vec<String>> =
        std::env::args_os().skip(1).map(|a| a.into_string().ok()).collect();
    let Some(args) = args else {
        eprintln!("sim-gui: a file name that is not valid UTF-8 cannot be opened");
        return ExitCode::from(2);
    };
    match args.first().map(String::as_str) {
        Some("--help" | "-h") => {
            print!("{}", HELP.replace("{version}", env!("CARGO_PKG_VERSION")));
            return ExitCode::SUCCESS;
        }
        Some("--version") => {
            println!("sim-gui {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    match sim_gui::app::run_window(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sim-gui: {e}");
            ExitCode::FAILURE
        }
    }
}

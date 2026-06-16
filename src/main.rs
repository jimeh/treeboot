use std::process::ExitCode;

const HELP: &str = "\
treeboot

Bootstrap new Git worktrees from one repo-local setup file.

Usage: treeboot [OPTIONS]

Options:
  -h, --help     Print help
  -V, --version  Print version
";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);

    match args.next().as_deref() {
        None => ExitCode::SUCCESS,
        Some("-h" | "--help") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some("-V" | "--version") => {
            println!("treeboot {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(arg) => {
            eprintln!("treeboot: unknown option: {arg}");
            eprintln!("Try 'treeboot --help' for usage.");
            ExitCode::from(2)
        }
    }
}

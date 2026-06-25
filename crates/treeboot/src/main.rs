use std::process::ExitCode;

mod commands;
mod reporter;

use commands::{parse, run_cli};
use reporter::StdoutReporter;

fn main() -> ExitCode {
    clap_complete::CompleteEnv::with_factory(commands::command).complete();

    let cli = parse();
    let mut reporter = StdoutReporter;

    match run_cli(cli, &mut reporter) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("treeboot: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

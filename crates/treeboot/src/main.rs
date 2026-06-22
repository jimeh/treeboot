use std::process::ExitCode;

use clap::{CommandFactory, Parser};

mod commands;
mod reporter;

use commands::{Cli, run_cli};
use reporter::StdoutReporter;

fn main() -> ExitCode {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();
    let mut reporter = StdoutReporter;

    match run_cli(cli, &mut reporter) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("treeboot: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

use clap::{Command as ClapCommand, CommandFactory, FromArgMatches, Parser, Subcommand};
use treeboot_core::Reporter;

mod check;
mod completions;
mod config;
mod doctor;
mod env;
mod init;
mod manual;
mod output;
mod run;
mod schema;
mod status;
mod version;

use check::CheckArgs;
use completions::CompletionsArgs;
use config::ConfigArgs;
use doctor::DoctorArgs;
use env::EnvArgs;
use init::InitArgs;
use manual::{CopyArgs, SymlinkArgs, SyncArgs};
use run::RunArgs;
use schema::SchemaArgs;
use status::StatusArgs;
use version::VersionArgs;

#[derive(Debug, Parser)]
#[command(
    name = "treeboot",
    about = "Bootstrap new Git worktrees from one repo-local setup file."
)]
pub(crate) struct Cli {
    #[command(flatten)]
    run: RunArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run worktree bootstrap.
    Run(RunArgs),
    /// Print worktree, root, and config discovery status.
    #[command(alias = "info")]
    Status(StatusArgs),
    /// Print version metadata.
    Version(VersionArgs),
    /// Copy files or directories from the root checkout.
    Copy(CopyArgs),
    /// Symlink files or directories from the root checkout.
    Symlink(SymlinkArgs),
    /// Sync files or directories from the root checkout.
    Sync(SyncArgs),
    /// Parse and print normalized config without executing it.
    Config(ConfigArgs),
    /// Validate bootstrap behavior without side effects.
    Check(CheckArgs),
    /// Create a starter config.
    Init(InitArgs),
    /// Print or write the bundled config JSON Schema.
    Schema(SchemaArgs),
    /// Diagnose treeboot discovery and validation.
    Doctor(DoctorArgs),
    /// Print child environment variables passed to configured commands.
    Env(EnvArgs),
    /// Print shell completion scripts.
    Completions(CompletionsArgs),
}

pub(crate) fn command() -> ClapCommand {
    Cli::command()
        .version(treeboot_core::treeboot_version_summary())
        .propagate_version(true)
}

pub(crate) fn parse() -> Cli {
    let matches = command().get_matches();
    Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit())
}

pub(crate) fn environment_input() -> treeboot_core::EnvironmentInput {
    treeboot_core::EnvironmentInput::from_process_env()
}

pub(crate) fn run_cli(cli: Cli, reporter: &mut dyn Reporter) -> treeboot_core::Result<()> {
    match cli.command {
        Some(Command::Run(args)) => treeboot_core::run(args.into(), reporter).map(|_| ()),
        Some(Command::Status(args)) => status::run_status_command(args),
        Some(Command::Version(args)) => version::run_version_command(args),
        Some(Command::Copy(args)) => {
            treeboot_core::run_file_operation(args.into_options(), reporter).map(|_| ())
        }
        Some(Command::Symlink(args)) => {
            treeboot_core::run_file_operation(args.into_options(), reporter).map(|_| ())
        }
        Some(Command::Sync(args)) => {
            treeboot_core::run_file_operation(args.into_options(), reporter).map(|_| ())
        }
        Some(Command::Config(args)) => config::run_config_command(args),
        Some(Command::Check(args)) => check::run_check_command(args),
        Some(Command::Init(args)) => treeboot_core::init(args.into(), reporter).map(|_| ()),
        Some(Command::Schema(args)) => schema::run_schema_command(args),
        Some(Command::Doctor(args)) => doctor::run_doctor_command(args),
        Some(Command::Env(args)) => env::run_env_command(args),
        Some(Command::Completions(args)) => completions::run_completions_command(args),
        None => treeboot_core::run(cli.run.into(), reporter).map(|_| ()),
    }
}

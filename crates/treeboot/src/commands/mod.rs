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
mod teardown;
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
use teardown::TeardownArgs;
use version::VersionArgs;

#[derive(Debug, Parser)]
#[command(
    name = "treeboot",
    about = "Bootstrap Git worktrees and run pre-removal teardown commands."
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
    /// Validate bootstrap and teardown behavior without side effects.
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
    /// Run configured teardown commands for a linked worktree.
    Teardown(TeardownArgs),
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

#[derive(Debug)]
pub(crate) enum CliError {
    Core(treeboot_core::Error),
    ConfirmationRequired,
    TeardownDeclined,
    PromptIo(std::io::Error),
}

impl From<treeboot_core::Error> for CliError {
    fn from(error: treeboot_core::Error) -> Self {
        Self::Core(error)
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core(error) => error.fmt(formatter),
            Self::ConfirmationRequired => {
                formatter.write_str("teardown confirmation requires a terminal; rerun with --yes")
            }
            Self::TeardownDeclined => formatter.write_str("teardown declined"),
            Self::PromptIo(error) => write!(formatter, "teardown confirmation failed: {error}"),
        }
    }
}

impl std::error::Error for CliError {}

impl CliError {
    pub(crate) const fn exit_code(&self) -> u8 {
        match self {
            Self::Core(error) => error.exit_code(),
            Self::ConfirmationRequired | Self::TeardownDeclined | Self::PromptIo(_) => 1,
        }
    }
}

pub(crate) fn run_cli(cli: Cli, reporter: &mut dyn Reporter) -> Result<(), CliError> {
    match cli.command {
        Some(Command::Run(args)) => treeboot_core::run(args.into(), reporter)
            .map(|_| ())
            .map_err(Into::into),
        Some(Command::Status(args)) => status::run_status_command(args).map_err(Into::into),
        Some(Command::Version(args)) => version::run_version_command(args).map_err(Into::into),
        Some(Command::Copy(args)) => {
            treeboot_core::run_file_operation(args.into_options(), reporter)
                .map(|_| ())
                .map_err(Into::into)
        }
        Some(Command::Symlink(args)) => {
            treeboot_core::run_file_operation(args.into_options(), reporter)
                .map(|_| ())
                .map_err(Into::into)
        }
        Some(Command::Sync(args)) => {
            treeboot_core::run_file_operation(args.into_options(), reporter)
                .map(|_| ())
                .map_err(Into::into)
        }
        Some(Command::Config(args)) => config::run_config_command(args).map_err(Into::into),
        Some(Command::Check(args)) => check::run_check_command(args).map_err(Into::into),
        Some(Command::Init(args)) => treeboot_core::init(args.into(), reporter)
            .map(|_| ())
            .map_err(Into::into),
        Some(Command::Schema(args)) => schema::run_schema_command(args).map_err(Into::into),
        Some(Command::Doctor(args)) => doctor::run_doctor_command(args).map_err(Into::into),
        Some(Command::Env(args)) => env::run_env_command(args).map_err(Into::into),
        Some(Command::Completions(args)) => {
            completions::run_completions_command(args).map_err(Into::into)
        }
        Some(Command::Teardown(args)) => teardown::run_teardown_command(args, reporter),
        None => treeboot_core::run(cli.run.into(), reporter)
            .map(|_| ())
            .map_err(Into::into),
    }
}

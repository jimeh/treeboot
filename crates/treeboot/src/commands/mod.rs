use clap::{Parser, Subcommand};
use treeboot_core::Reporter;

mod completions;
mod config;
mod init;
mod manual;
mod run;
mod status;

use completions::CompletionsArgs;
use config::ConfigArgs;
use init::InitArgs;
use manual::{CopyArgs, SymlinkArgs, SyncArgs};
use run::RunArgs;
use status::StatusArgs;

#[derive(Debug, Parser)]
#[command(
    name = "treeboot",
    version,
    about = "Bootstrap new Git worktrees from one repo-local setup file.",
    propagate_version = true
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
    /// Print worktree, root, config, and init script discovery status.
    #[command(alias = "info")]
    Status(StatusArgs),
    /// Copy files or directories from the root checkout.
    Copy(CopyArgs),
    /// Symlink files or directories from the root checkout.
    Symlink(SymlinkArgs),
    /// Sync files or directories from the root checkout.
    Sync(SyncArgs),
    /// Parse and print normalized config without executing it.
    Config(ConfigArgs),
    /// Create a starter config or init script.
    Init(InitArgs),
    /// Print shell completion scripts.
    Completions(CompletionsArgs),
}

pub(crate) fn run_cli(cli: Cli, reporter: &mut dyn Reporter) -> treeboot_core::Result<()> {
    match cli.command {
        Some(Command::Run(args)) => treeboot_core::run(args.into(), reporter).map(|_| ()),
        Some(Command::Status(args)) => status::run_status_command(args),
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
        Some(Command::Init(args)) => treeboot_core::init(args.into(), reporter).map(|_| ()),
        Some(Command::Completions(args)) => completions::run_completions_command(args),
        None => treeboot_core::run(cli.run.into(), reporter).map(|_| ()),
    }
}

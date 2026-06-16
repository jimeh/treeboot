use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use treeboot_core::{InitKind, InitOptions, OutputEvent, Reporter, RunOptions};

#[derive(Debug, Parser)]
#[command(
    name = "treeboot",
    version,
    about = "Bootstrap new Git worktrees from one repo-local setup file.",
    propagate_version = true
)]
struct Cli {
    #[command(flatten)]
    run: RunArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run worktree bootstrap.
    Run(RunArgs),
    /// Create a starter config or init script.
    Init(InitArgs),
}

#[derive(Debug, Args, Clone, Default)]
struct RunArgs {
    /// Override the checkout used as the file-operation source.
    #[arg(long)]
    root: Option<PathBuf>,

    /// Use one specific config file and skip init script discovery.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Fail on missing config and stricter file-operation conflicts.
    #[arg(long)]
    strict: bool,

    /// Replace existing file-operation targets where supported.
    #[arg(long)]
    force: bool,

    /// Print planned work without changing files or running commands.
    #[arg(long)]
    dry_run: bool,

    /// Run file operations only.
    #[arg(long)]
    no_commands: bool,
}

#[derive(Debug, Args, Clone, Default)]
struct InitArgs {
    /// Create a starter TOML config.
    #[arg(long, conflicts_with = "script")]
    config: bool,

    /// Create an executable init script.
    #[arg(long, conflicts_with = "config")]
    script: bool,

    /// Output path for the generated file.
    #[arg(long)]
    path: Option<PathBuf>,

    /// Replace an existing init output file.
    #[arg(long)]
    force: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut reporter = StdoutReporter;
    let result = match cli.command {
        Some(Command::Run(args)) => treeboot_core::run(args.into(), &mut reporter).map(|_| ()),
        Some(Command::Init(args)) => treeboot_core::init(args.into(), &mut reporter).map(|_| ()),
        None => treeboot_core::run(cli.run.into(), &mut reporter).map(|_| ()),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("treeboot: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

impl From<RunArgs> for RunOptions {
    fn from(args: RunArgs) -> Self {
        Self {
            cwd: None,
            root: args.root,
            config: args.config,
            strict: args.strict,
            force: args.force,
            dry_run: args.dry_run,
            no_commands: args.no_commands,
        }
    }
}

impl From<InitArgs> for InitOptions {
    fn from(args: InitArgs) -> Self {
        Self {
            cwd: None,
            kind: init_kind(&args),
            path: args.path,
            force: args.force,
        }
    }
}

fn init_kind(args: &InitArgs) -> Option<InitKind> {
    match (args.config, args.script) {
        (true, false) => Some(InitKind::Config),
        (false, true) => Some(InitKind::Script),
        _ => None,
    }
}

struct StdoutReporter;

impl Reporter for StdoutReporter {
    fn report(&mut self, event: OutputEvent) -> std::io::Result<()> {
        println!("{}", event.message());
        Ok(())
    }
}

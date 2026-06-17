use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use treeboot_core::{
    CommandKind, CommandOperation, ConfigOptions, ConfigReport, Error, FileOperation,
    FileOperationKind, InitKind, InitOptions, OutputEvent, Reporter, RunOptions, RunPlanOptions,
};

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
    /// Parse and print normalized config without executing it.
    Config(ConfigArgs),
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

#[derive(Debug, Args, Clone, Default)]
struct ConfigArgs {
    /// Override the checkout used as the file-operation source.
    #[arg(long)]
    root: Option<PathBuf>,

    /// Use one specific config file and skip config discovery.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Output format for normalized config.
    #[arg(long, value_enum, default_value_t = ConfigFormat::Text)]
    format: ConfigFormat,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum ConfigFormat {
    #[default]
    Text,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut reporter = StdoutReporter;
    let result = match cli.command {
        Some(Command::Run(args)) => treeboot_core::run(args.into(), &mut reporter).map(|_| ()),
        Some(Command::Config(args)) => run_config_command(args),
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

fn run_config_command(args: ConfigArgs) -> treeboot_core::Result<()> {
    let format = args.format;
    let report = treeboot_core::inspect_config(args.into())?;

    match format {
        ConfigFormat::Text => print_config_text(&report).map_err(|source| Error::Output { source }),
        ConfigFormat::Json => {
            let output = serde_json::json!({
                "path": report.path,
                "config": report.config,
            });
            let stdout = std::io::stdout();
            let mut handle = stdout.lock();

            serde_json::to_writer_pretty(&mut handle, &output).map_err(|source| Error::Output {
                source: std::io::Error::other(source),
            })?;
            use std::io::Write;
            writeln!(handle).map_err(|source| Error::Output { source })
        }
    }?;

    if let Err(error) = treeboot_core::plan_run_config(
        &report.path,
        &report.config,
        &report.context,
        RunPlanOptions::default(),
    ) {
        eprintln!("treeboot: warning: run validation would fail: {error}");
    }

    Ok(())
}

fn print_config_text(report: &ConfigReport) -> std::io::Result<()> {
    println!("treeboot: config {}", report.path.display());
    println!();
    println!("files:");
    if report.config.files.is_empty() {
        println!("  (none)");
    } else {
        for operation in &report.config.files {
            println!("  {}", file_operation_summary(operation));
        }
    }
    println!();
    println!("commands:");
    if report.config.commands.is_empty() {
        println!("  (none)");
    } else {
        for command in &report.config.commands {
            println!("  {}", command_summary(command));
        }
    }

    Ok(())
}

fn file_operation_summary(operation: &FileOperation) -> String {
    let mut summary = format!(
        "{} {} -> {}",
        file_operation_name(operation.operation),
        operation.source.display(),
        operation.target.display()
    );

    if operation.required {
        summary.push_str(" required=true");
    }
    if let Some(compare) = operation.compare {
        summary.push_str(&format!(" compare={compare:?}").to_lowercase());
    }
    if let Some(delete_extra) = operation.delete_extra {
        summary.push_str(&format!(" delete_extra={delete_extra}"));
    }

    summary
}

fn file_operation_name(operation: FileOperationKind) -> &'static str {
    match operation {
        FileOperationKind::Copy => "copy",
        FileOperationKind::Symlink => "symlink",
        FileOperationKind::Sync => "sync",
    }
}

fn command_summary(command: &CommandOperation) -> String {
    let mut summary = match &command.command {
        CommandKind::Shell { run } => format!("run {run:?}"),
        CommandKind::Direct { program, args } => {
            let mut parts = vec![program.as_str()];
            parts.extend(args.iter().map(String::as_str));
            format!("exec {}", parts.join(" "))
        }
    };

    if command.async_command {
        summary.push_str(" async=true");
    }
    if command.allow_failure {
        summary.push_str(" allow_failure=true");
    }
    if let Some(cwd) = &command.cwd {
        summary.push_str(&format!(" cwd={}", cwd.display()));
    }

    summary
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

impl From<ConfigArgs> for ConfigOptions {
    fn from(args: ConfigArgs) -> Self {
        Self {
            cwd: None,
            root: args.root,
            config: args.config,
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

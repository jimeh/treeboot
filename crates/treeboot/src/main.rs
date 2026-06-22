use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum, ValueHint};
use clap_complete::{ArgValueCompleter, CompletionCandidate, Shell};
use treeboot_core::{
    CommandKind, CommandOperation, ConfigOptions, ConfigReport, Error, FileOperation,
    FileOperationCompletionOptions, FileOperationKind, FileOperationOptions, InitKind, InitOptions,
    OutputEvent, Reporter, RunOptions, RuntimeOptionOverrides, SymlinkMode, SyncCompare,
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

#[derive(Debug, Args, Clone, Default)]
struct ManualArgs {
    /// Override the checkout used as the file-operation source.
    #[arg(short, long, value_hint = ValueHint::DirPath)]
    root: Option<PathBuf>,

    /// Target path in the current worktree.
    #[arg(short, long, value_hint = ValueHint::AnyPath)]
    target: Option<PathBuf>,

    /// Fail when a source is missing.
    #[arg(long)]
    required: bool,

    /// Fail on stricter file-operation conflicts.
    #[arg(short = 'S', long)]
    strict: bool,

    /// Replace existing file-operation targets where supported.
    #[arg(short, long)]
    force: bool,

    /// Print planned work without changing files.
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Source paths from the root checkout.
    #[arg(
        required = true,
        num_args = 1..,
        value_hint = ValueHint::AnyPath,
        add = ArgValueCompleter::new(root_source_completer),
    )]
    sources: Vec<PathBuf>,
}

#[derive(Debug, Args, Clone, Default)]
struct CopyArgs {
    #[command(flatten)]
    manual: ManualArgs,

    /// How to handle source symlinks.
    #[arg(long, value_enum)]
    symlinks: Option<CliSymlinkMode>,
}

#[derive(Debug, Args, Clone, Default)]
struct SymlinkArgs {
    #[command(flatten)]
    manual: ManualArgs,
}

#[derive(Debug, Args, Clone, Default)]
struct SyncArgs {
    #[command(flatten)]
    manual: ManualArgs,

    /// How to handle source symlinks.
    #[arg(long, value_enum)]
    symlinks: Option<CliSymlinkMode>,

    /// How to compare source and target files.
    #[arg(long, value_enum)]
    compare: Option<CliSyncCompare>,

    /// Delete target-only files.
    #[arg(short = 'D', long, conflicts_with = "no_delete")]
    delete: bool,

    /// Preserve target-only files.
    #[arg(long, conflicts_with = "delete")]
    no_delete: bool,
}

#[derive(Debug, Args, Clone, Default)]
struct RunArgs {
    /// Override the checkout used as the file-operation source.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Use one specific config file and skip init script discovery.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Fail on missing config and stricter file-operation conflicts.
    #[arg(short = 'S', long)]
    strict: bool,

    /// Replace existing file-operation targets where supported.
    #[arg(short, long)]
    force: bool,

    /// Print planned work without changing files or running commands.
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Run file operations only.
    #[arg(long)]
    skip_commands: bool,
}

#[derive(Debug, Args, Clone, Default)]
struct InitArgs {
    /// Create a starter TOML config.
    #[arg(long, conflicts_with = "script")]
    config: bool,

    /// Create an executable init script.
    #[arg(short, long, conflicts_with = "config")]
    script: bool,

    /// Output path for the generated file.
    #[arg(short, long)]
    path: Option<PathBuf>,
}

#[derive(Debug, Args, Clone, Default)]
struct ConfigArgs {
    /// Override the checkout used as the file-operation source.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Use one specific config file and skip config discovery.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Output format for normalized config.
    #[arg(short = 'o', long, value_enum, conflicts_with = "json")]
    format: Option<ConfigFormat>,

    /// Print normalized config as JSON.
    #[arg(short = 'J', long)]
    json: bool,
}

#[derive(Debug, Args, Clone, Copy)]
struct CompletionsArgs {
    /// Shell to generate completions for.
    shell: Shell,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum ConfigFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliSymlinkMode {
    Preserve,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliSyncCompare {
    Metadata,
    Checksum,
}

fn main() -> ExitCode {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();
    let mut reporter = StdoutReporter;
    let result = match cli.command {
        Some(Command::Run(args)) => treeboot_core::run(args.into(), &mut reporter).map(|_| ()),
        Some(Command::Copy(args)) => {
            treeboot_core::run_file_operation(args.into_options(), &mut reporter).map(|_| ())
        }
        Some(Command::Symlink(args)) => {
            treeboot_core::run_file_operation(args.into_options(), &mut reporter).map(|_| ())
        }
        Some(Command::Sync(args)) => {
            treeboot_core::run_file_operation(args.into_options(), &mut reporter).map(|_| ())
        }
        Some(Command::Config(args)) => run_config_command(args),
        Some(Command::Init(args)) => treeboot_core::init(args.into(), &mut reporter).map(|_| ()),
        Some(Command::Completions(args)) => run_completions_command(args),
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

fn root_source_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let root = completion_root_override();
    treeboot_core::file_operation_source_candidates(FileOperationCompletionOptions {
        cwd: std::env::current_dir().ok(),
        root,
        current: PathBuf::from(current),
    })
    .into_iter()
    .map(CompletionCandidate::new)
    .collect()
}

fn completion_root_override() -> Option<PathBuf> {
    let mut args = std::env::args_os();
    for arg in args.by_ref() {
        if arg == "--" {
            break;
        }
    }

    while let Some(arg) = args.next() {
        if let Some(value) = arg.to_string_lossy().strip_prefix("--root=") {
            return Some(PathBuf::from(value));
        }
        if arg == "--root" || arg == "-r" {
            return args.next().map(PathBuf::from);
        }
    }

    None
}

fn run_completions_command(args: CompletionsArgs) -> treeboot_core::Result<()> {
    let shells = clap_complete::env::Shells::builtins();
    let shell = shells
        .completer(&args.shell.to_string())
        .ok_or_else(|| Error::Output {
            source: std::io::Error::other(format!("unsupported shell {}", args.shell)),
        })?;
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let completer = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "treeboot".to_owned());

    shell
        .write_registration("COMPLETE", "treeboot", "treeboot", &completer, &mut handle)
        .map_err(|source| Error::Output { source })
}

fn run_config_command(args: ConfigArgs) -> treeboot_core::Result<()> {
    let format = args.output_format();
    let env_options = RuntimeOptionOverrides::from_env()?;
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

    let plan_options = env_options.resolve(&report.config.options, false);

    if let Err(error) = treeboot_core::ActionPlan::from_manifest(
        &report.path,
        &report.config,
        &report.context,
        plan_options.into(),
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
        operation.operation.as_str(),
        operation.source.display(),
        operation.target.display()
    );

    if operation.required {
        summary.push_str(" required=true");
    }
    if let Some(compare) = operation.compare {
        summary.push_str(&format!(" compare={compare:?}").to_lowercase());
    }
    if let Some(delete) = operation.delete {
        summary.push_str(&format!(" delete={delete}"));
    }
    if let Some(symlinks) = operation.symlinks {
        summary.push_str(&format!(" symlinks={symlinks:?}").to_lowercase());
    }

    summary
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

    if command.allow_failure {
        summary.push_str(" allow_failure=true");
    }
    if let Some(cwd) = &command.cwd {
        summary.push_str(&format!(" cwd={}", cwd.display()));
    }
    if !command.env.is_empty() {
        let env = command
            .env
            .iter()
            .map(|(name, value)| format!("{name}={value:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        summary.push_str(&format!(" env={{{env}}}"));
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
            skip_commands: args.skip_commands,
        }
    }
}

impl CopyArgs {
    fn into_options(self) -> FileOperationOptions {
        FileOperationOptions {
            symlinks: self.symlinks.map(Into::into),
            ..self.manual.into_options(FileOperationKind::Copy)
        }
    }
}

impl SymlinkArgs {
    fn into_options(self) -> FileOperationOptions {
        self.manual.into_options(FileOperationKind::Symlink)
    }
}

impl SyncArgs {
    fn into_options(self) -> FileOperationOptions {
        FileOperationOptions {
            symlinks: self.symlinks.map(Into::into),
            compare: self.compare.map(Into::into),
            delete: sync_delete_option(self.delete, self.no_delete),
            ..self.manual.into_options(FileOperationKind::Sync)
        }
    }
}

impl ManualArgs {
    fn into_options(self, operation: FileOperationKind) -> FileOperationOptions {
        FileOperationOptions {
            cwd: None,
            root: self.root,
            operation,
            sources: self.sources,
            target: self.target,
            required: self.required,
            symlinks: None,
            compare: None,
            delete: None,
            strict: self.strict,
            force: self.force,
            dry_run: self.dry_run,
        }
    }
}

const fn sync_delete_option(delete: bool, no_delete: bool) -> Option<bool> {
    if delete {
        Some(true)
    } else if no_delete {
        Some(false)
    } else {
        None
    }
}

impl From<CliSymlinkMode> for SymlinkMode {
    fn from(value: CliSymlinkMode) -> Self {
        match value {
            CliSymlinkMode::Preserve => Self::Preserve,
        }
    }
}

impl From<CliSyncCompare> for SyncCompare {
    fn from(value: CliSyncCompare) -> Self {
        match value {
            CliSyncCompare::Metadata => Self::Metadata,
            CliSyncCompare::Checksum => Self::Checksum,
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

impl ConfigArgs {
    fn output_format(&self) -> ConfigFormat {
        if self.json {
            ConfigFormat::Json
        } else {
            self.format.unwrap_or_default()
        }
    }
}

impl From<InitArgs> for InitOptions {
    fn from(args: InitArgs) -> Self {
        Self {
            cwd: None,
            kind: init_kind(&args),
            path: args.path,
        }
    }
}

fn init_kind(args: &InitArgs) -> InitKind {
    match (args.config, args.script) {
        (false, true) => InitKind::Script,
        _ => InitKind::Config,
    }
}

struct StdoutReporter;

impl Reporter for StdoutReporter {
    fn report(&mut self, event: OutputEvent) -> std::io::Result<()> {
        println!("{}", event.message());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual_args() -> ManualArgs {
        ManualArgs {
            sources: vec![PathBuf::from(".env")],
            ..ManualArgs::default()
        }
    }

    #[test]
    fn sync_delete_option_should_normalize_delete_flags() {
        assert_eq!(sync_delete_option(false, false), None);
        assert_eq!(sync_delete_option(true, false), Some(true));
        assert_eq!(sync_delete_option(false, true), Some(false));
        assert_eq!(sync_delete_option(true, true), Some(true));
    }

    #[test]
    fn copy_args_should_preserve_manual_options_and_symlink_mode() {
        let args = CopyArgs {
            manual: ManualArgs {
                target: Some(PathBuf::from("local")),
                required: true,
                strict: true,
                force: true,
                dry_run: true,
                ..manual_args()
            },
            symlinks: Some(CliSymlinkMode::Preserve),
        };

        let options = args.into_options();

        assert_eq!(options.operation, FileOperationKind::Copy);
        assert_eq!(options.target, Some(PathBuf::from("local")));
        assert!(options.required);
        assert!(options.strict);
        assert!(options.force);
        assert!(options.dry_run);
        assert_eq!(options.symlinks, Some(SymlinkMode::Preserve));
    }

    #[test]
    fn symlink_args_should_set_symlink_operation() {
        let options = SymlinkArgs {
            manual: manual_args(),
        }
        .into_options();

        assert_eq!(options.operation, FileOperationKind::Symlink);
    }

    #[test]
    fn sync_args_should_normalize_sync_specific_options() {
        let options = SyncArgs {
            manual: manual_args(),
            symlinks: Some(CliSymlinkMode::Preserve),
            compare: Some(CliSyncCompare::Checksum),
            delete: true,
            no_delete: false,
        }
        .into_options();

        assert_eq!(options.operation, FileOperationKind::Sync);
        assert_eq!(options.symlinks, Some(SymlinkMode::Preserve));
        assert_eq!(options.compare, Some(SyncCompare::Checksum));
        assert_eq!(options.delete, Some(true));
    }

    #[test]
    fn cli_value_enums_should_convert_to_core_options() {
        assert_eq!(
            SymlinkMode::from(CliSymlinkMode::Preserve),
            SymlinkMode::Preserve
        );
        assert_eq!(
            SyncCompare::from(CliSyncCompare::Metadata),
            SyncCompare::Metadata
        );
        assert_eq!(
            SyncCompare::from(CliSyncCompare::Checksum),
            SyncCompare::Checksum
        );
    }
}

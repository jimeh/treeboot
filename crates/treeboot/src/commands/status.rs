use std::io::Write;
use std::path::PathBuf;

use clap::Args;
use treeboot_core::{Error, InitScriptStatus, StatusOptions, StatusReport, StatusSnapshotReport};

use super::output::{OutputArgs, ReportFormat, write_structured};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct StatusArgs {
    /// Override the checkout used for status discovery.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Use one specific config file and skip init script discovery.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Skip init script discovery and use declarative config discovery.
    #[arg(long)]
    no_init_script: bool,

    #[command(flatten)]
    output: OutputArgs,
}

pub(crate) fn run_status_command(args: StatusArgs) -> treeboot_core::Result<()> {
    let format = args.output.format();
    let report = treeboot_core::inspect_status(args.into())?;

    match format {
        ReportFormat::Text => print_status_text(&report).map_err(|source| Error::Output { source }),
        format => write_structured(&StatusSnapshotReport::from(&report), format),
    }
}

fn print_status_text(report: &StatusReport) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    writeln!(handle, "treeboot: status")?;
    writeln!(
        handle,
        "worktree: {}",
        report.context.worktree_path.display()
    )?;
    writeln!(handle, "root: {}", report.context.root_path.display())?;
    let default_branch = if report.context.default_branch.is_empty() {
        "(unknown)"
    } else {
        &report.context.default_branch
    };
    writeln!(handle, "default_branch: {default_branch}")?;

    match &report.init_script {
        InitScriptStatus::Skipped => writeln!(handle, "init_script: (skipped)")?,
        InitScriptStatus::Found { path } => {
            writeln!(handle, "init_script: {}", path.display())?;
        }
        InitScriptStatus::NotFound { ignored } => {
            writeln!(handle, "init_script: (none)")?;
            for ignored in ignored {
                writeln!(handle, "ignored_init_script: {}", ignored.path.display())?;
            }
        }
    }

    if let Some(path) = &report.config {
        writeln!(handle, "config: {}", path.display())
    } else {
        writeln!(handle, "config: (none)")
    }
}

impl From<StatusArgs> for StatusOptions {
    fn from(args: StatusArgs) -> Self {
        Self {
            cwd: None,
            root: args.root,
            config: args.config,
            no_init_script: args.no_init_script,
        }
    }
}

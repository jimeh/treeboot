use std::io::Write;
use std::path::PathBuf;

use clap::Args;
use treeboot_core::{Error, StatusOptions, StatusReport, StatusSnapshotReport};

use super::environment_input;
use super::output::{OutputArgs, ReportFormat, write_structured};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct StatusArgs {
    /// Override the checkout used for status discovery.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Use one specific config file instead of config discovery.
    #[arg(short, long)]
    config: Option<PathBuf>,

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
            environment: environment_input(),
            config: args.config,
        }
    }
}

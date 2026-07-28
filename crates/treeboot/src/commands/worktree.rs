use std::io::Write;

use clap::{Args, Subcommand};
use treeboot_core::{
    Error, WorktreeInspectionOptions, WorktreeListReport, WorktreePathReport, inspect_worktree_id,
    inspect_worktree_list, inspect_worktree_path,
};

use super::environment_input;
use super::output::{OutputArgs, ReportFormat, write_structured};

#[derive(Debug, Args)]
pub(crate) struct WorktreeArgs {
    #[command(subcommand)]
    command: WorktreeCommand,
}

#[derive(Debug, Subcommand)]
enum WorktreeCommand {
    /// Print the current worktree's Treeboot identifier.
    Id(WorktreeIdArgs),
    /// Resolve a Treeboot worktree identifier to its path.
    Path(WorktreePathArgs),
    /// List worktree identifiers and paths.
    List(WorktreeListArgs),
}

#[derive(Debug, Args)]
struct WorktreeIdArgs {
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct WorktreePathArgs {
    /// Complete Treeboot worktree identifier.
    id: String,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct WorktreeListArgs {
    #[command(flatten)]
    output: OutputArgs,
}

pub(crate) fn run_worktree_command(args: WorktreeArgs) -> treeboot_core::Result<()> {
    match args.command {
        WorktreeCommand::Id(args) => {
            let format = args.output.format();
            let report = inspect_worktree_id(options())?;
            match format {
                ReportFormat::Text => print_value(&report.id),
                format => write_structured(&report, format),
            }
        }
        WorktreeCommand::Path(args) => {
            let format = args.output.format();
            let report = inspect_worktree_path(&args.id, options())?;
            match format {
                ReportFormat::Text => print_path(&report),
                format => write_structured(&report, format),
            }
        }
        WorktreeCommand::List(args) => {
            let format = args.output.format();
            let report = inspect_worktree_list(options())?;
            match format {
                ReportFormat::Text => print_list(&report),
                format => write_structured(&report, format),
            }
        }
    }
}

fn options() -> WorktreeInspectionOptions {
    let mut options = WorktreeInspectionOptions::default();
    options.environment = environment_input();
    options
}

fn print_value(value: &str) -> treeboot_core::Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{value}").map_err(|source| Error::Output { source })
}

fn print_path(report: &WorktreePathReport) -> treeboot_core::Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{}", report.path.display()).map_err(|source| Error::Output { source })
}

fn print_list(report: &WorktreeListReport) -> treeboot_core::Result<()> {
    let id_width = report
        .worktrees
        .iter()
        .map(|entry| entry.id.len())
        .chain([2])
        .max()
        .unwrap_or(2);
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{:<id_width$}  PATH", "ID").map_err(|source| Error::Output { source })?;
    for entry in &report.worktrees {
        writeln!(handle, "{:<id_width$}  {}", entry.id, entry.path.display())
            .map_err(|source| Error::Output { source })?;
    }
    Ok(())
}

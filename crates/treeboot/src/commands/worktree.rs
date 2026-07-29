use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::Path;

use clap::builder::TypedValueParser;
use clap::error::ErrorKind;
use clap::{Args, Subcommand, ValueHint};
use treeboot_core::{
    Error, WorktreeIdentityOptions, WorktreeInspectionOptions, WorktreeListReport,
    WorktreePathReport, inspect_worktree_id, inspect_worktree_list, inspect_worktree_path,
    inspect_worktree_slug,
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
    /// Print a worktree's compact Treeboot ID.
    Id(WorktreeIdArgs),
    /// Print a worktree's readable Treeboot slug.
    Slug(WorktreeSlugArgs),
    /// Resolve an exact Treeboot worktree ID to its path.
    Path(WorktreePathArgs),
    /// List worktree IDs, slugs, and paths.
    List(WorktreeListArgs),
}

#[derive(Debug, Args)]
struct WorktreeIdArgs {
    /// Exact target directory whose ID should be derived.
    #[arg(
        value_hint = ValueHint::DirPath,
        value_parser = NonEmptyPathValueParser
    )]
    path: Option<OsString>,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct WorktreeSlugArgs {
    /// Exact target directory whose slug should be derived.
    #[arg(
        value_hint = ValueHint::DirPath,
        value_parser = NonEmptyPathValueParser
    )]
    path: Option<OsString>,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct WorktreePathArgs {
    /// Complete Treeboot worktree ID.
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
            let report = inspect_worktree_id(identity_options(args.path))?;
            match format {
                ReportFormat::Text => print_value(&report.id),
                format => write_structured(&report, format),
            }
        }
        WorktreeCommand::Slug(args) => {
            let format = args.output.format();
            let report = inspect_worktree_slug(identity_options(args.path))?;
            match format {
                ReportFormat::Text => print_value(&report.slug),
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

fn identity_options(path: Option<OsString>) -> WorktreeIdentityOptions {
    let mut options = WorktreeIdentityOptions::default();
    options.path = path.map(Into::into);
    options.environment = environment_input();
    options
}

#[derive(Clone)]
struct NonEmptyPathValueParser;

impl TypedValueParser for NonEmptyPathValueParser {
    type Value = OsString;

    fn parse_ref(
        &self,
        _command: &clap::Command,
        _argument: Option<&clap::Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, clap::Error> {
        if value.is_empty() {
            Err(clap::Error::raw(
                ErrorKind::InvalidValue,
                "PATH must not be empty",
            ))
        } else {
            Ok(value.to_os_string())
        }
    }
}

fn print_value(value: &str) -> treeboot_core::Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{value}").map_err(|source| Error::Output { source })
}

fn print_path(report: &WorktreePathReport) -> treeboot_core::Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    write_path_line(&mut handle, &report.path).map_err(|source| Error::Output { source })
}

fn print_list(report: &WorktreeListReport) -> treeboot_core::Result<()> {
    let id_width = report
        .worktrees
        .iter()
        .map(|entry| entry.id.len())
        .chain([2])
        .max()
        .unwrap_or(2);
    let slug_width = report
        .worktrees
        .iter()
        .map(|entry| entry.slug.len())
        .chain([4])
        .max()
        .unwrap_or(4);
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{:<id_width$}  {:<slug_width$}  PATH", "ID", "SLUG")
        .map_err(|source| Error::Output { source })?;
    for entry in &report.worktrees {
        write!(
            handle,
            "{:<id_width$}  {:<slug_width$}  ",
            entry.id, entry.slug
        )
        .map_err(|source| Error::Output { source })?;
        write_path_line(&mut handle, &entry.path).map_err(|source| Error::Output { source })?;
    }
    Ok(())
}

#[cfg(unix)]
fn write_path_line(writer: &mut dyn Write, path: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    writer.write_all(path.as_os_str().as_bytes())?;
    writer.write_all(b"\n")
}

#[cfg(not(unix))]
fn write_path_line(writer: &mut dyn Write, path: &Path) -> std::io::Result<()> {
    writeln!(writer, "{}", path.display())
}

use std::ffi::OsStr;
use std::path::PathBuf;

use clap::{Args, ValueEnum, ValueHint};
use clap_complete::{ArgValueCompleter, CompletionCandidate};
use treeboot_core::{
    FileOperationCompletionOptions, FileOperationKind, FileOperationOptions, SymlinkMode,
    SyncCompare,
};

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
pub(crate) struct CopyArgs {
    #[command(flatten)]
    manual: ManualArgs,

    /// How to handle source symlinks.
    #[arg(long, value_enum)]
    symlinks: Option<CliSymlinkMode>,
}

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct SymlinkArgs {
    #[command(flatten)]
    manual: ManualArgs,
}

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct SyncArgs {
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliSymlinkMode {
    Preserve,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliSyncCompare {
    Metadata,
    Checksum,
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

impl CopyArgs {
    pub(crate) fn into_options(self) -> FileOperationOptions {
        FileOperationOptions {
            symlinks: self.symlinks.map(Into::into),
            ..self.manual.into_options(FileOperationKind::Copy)
        }
    }
}

impl SymlinkArgs {
    pub(crate) fn into_options(self) -> FileOperationOptions {
        self.manual.into_options(FileOperationKind::Symlink)
    }
}

impl SyncArgs {
    pub(crate) fn into_options(self) -> FileOperationOptions {
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

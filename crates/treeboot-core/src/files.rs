use std::collections::BTreeSet;
use std::fs::{self, File, FileTimes, Metadata};
use std::path::{Component, Path, PathBuf};

use crate::{
    Error, FileOperationKind, OutputEvent, PlannedFileOperation, PlannedFileStatus, Reporter,
    Result, RunPlan,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileApplyOptions {
    pub(crate) strict: bool,
    pub(crate) force: bool,
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileApplyReport {
    pub(crate) action_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FileAction {
    CreateDirectory {
        operation: FileOperationKind,
        source: PathBuf,
        target: PathBuf,
        target_path: PathBuf,
    },
    CopyFile {
        operation: FileOperationKind,
        source: PathBuf,
        target: PathBuf,
        source_path: PathBuf,
        target_path: PathBuf,
        replace: bool,
    },
    CreateSymlink {
        operation: FileOperationKind,
        source: PathBuf,
        target: PathBuf,
        target_path: PathBuf,
        link_target: PathBuf,
        final_target: PathBuf,
        target_is_dir: bool,
        replace: bool,
    },
    Skip {
        operation: FileOperationKind,
        target: PathBuf,
        reason: String,
    },
    Warning {
        path: PathBuf,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy)]
struct CopyEntry<'a> {
    source_path: &'a Path,
    target_path: &'a Path,
    source: &'a Path,
    target: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SymlinkActionPlan {
    operation: FileOperationKind,
    source: PathBuf,
    target: PathBuf,
    target_path: PathBuf,
    link_target: PathBuf,
    final_target: PathBuf,
    target_is_dir: bool,
}

pub(crate) fn apply_file_operations(
    plan: &RunPlan,
    options: FileApplyOptions,
    reporter: &mut dyn Reporter,
) -> Result<FileApplyReport> {
    ensure_supported(plan)?;

    let mut actions = Vec::new();
    for operation in &plan.files {
        plan_operation(plan, operation, options, &mut actions)?;
    }
    add_symlink_warnings(&mut actions);

    for action in &actions {
        if options.dry_run {
            report_dry_run(action, reporter)?;
        } else {
            apply_action(action, reporter)?;
        }
    }

    Ok(FileApplyReport {
        action_count: actions.len(),
    })
}

fn ensure_supported(plan: &RunPlan) -> Result<()> {
    if plan.files.iter().any(|operation| {
        operation.operation == FileOperationKind::Sync
            && operation.status == PlannedFileStatus::Ready
    }) {
        return Err(Error::SyncExecutionNotImplemented(plan.config_path.clone()));
    }

    Ok(())
}

fn plan_operation(
    plan: &RunPlan,
    operation: &PlannedFileOperation,
    options: FileApplyOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    if operation.status == PlannedFileStatus::SkippedMissingSource {
        actions.push(FileAction::Skip {
            operation: operation.operation,
            target: operation.target.clone(),
            reason: "missing source".to_owned(),
        });
        return Ok(());
    }

    match operation.operation {
        FileOperationKind::Copy => plan_copy(plan, operation, options, actions),
        FileOperationKind::Symlink => plan_symlink(operation, options, actions),
        FileOperationKind::Sync => Ok(()),
    }
}

fn plan_copy(
    plan: &RunPlan,
    operation: &PlannedFileOperation,
    options: FileApplyOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    let source_path = raw_source_path(plan, operation);
    let metadata = metadata(&source_path, operation.operation)?;

    if metadata.file_type().is_symlink() {
        return plan_copy_symlink(plan, operation, &source_path, options, actions);
    }
    if metadata.is_file() {
        return plan_copy_file(
            operation,
            operation.source.clone(),
            operation.target.clone(),
            &source_path,
            &operation.target_path,
            options,
            actions,
        );
    }
    if metadata.is_dir() {
        return plan_copy_directory(
            plan,
            operation,
            CopyEntry {
                source_path: &source_path,
                target_path: &operation.target_path,
                source: &operation.source,
                target: &operation.target,
            },
            options,
            actions,
        );
    }

    conflict(
        operation.operation,
        source_path,
        "source file type is unsupported",
    )
}

fn plan_copy_directory(
    plan: &RunPlan,
    operation: &PlannedFileOperation,
    entry: CopyEntry<'_>,
    options: FileApplyOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    match maybe_metadata(entry.target_path, operation.operation)? {
        Some(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            return conflict(
                operation.operation,
                entry.target_path.to_path_buf(),
                "target is a file or symlink",
            );
        }
        Some(metadata) if metadata.is_dir() => {
            if options.strict {
                return conflict(
                    operation.operation,
                    entry.target_path.to_path_buf(),
                    "target directory exists",
                );
            }

            if !options.force {
                actions.push(FileAction::Skip {
                    operation: operation.operation,
                    target: entry.target.to_path_buf(),
                    reason: "target directory exists".to_owned(),
                });
            }
        }
        Some(_) => {
            return conflict(
                operation.operation,
                entry.target_path.to_path_buf(),
                "target file type is unsupported",
            );
        }
        None => actions.push(FileAction::CreateDirectory {
            operation: operation.operation,
            source: entry.source.to_path_buf(),
            target: entry.target.to_path_buf(),
            target_path: entry.target_path.to_path_buf(),
        }),
    }

    for child in fs::read_dir(entry.source_path).map_err(|source| Error::FileOperationIo {
        operation: operation_name(operation.operation),
        path: entry.source_path.to_path_buf(),
        source,
    })? {
        let child = child.map_err(|source| Error::FileOperationIo {
            operation: operation_name(operation.operation),
            path: entry.source_path.to_path_buf(),
            source,
        })?;
        let child_source_path = child.path();
        let child_target_path = entry.target_path.join(child.file_name());
        let child_source = entry.source.join(child.file_name());
        let child_target = entry.target.join(child.file_name());
        let child_metadata = metadata(&child_source_path, operation.operation)?;

        if child_metadata.file_type().is_symlink() {
            plan_copy_symlink_at(
                plan,
                operation,
                CopyEntry {
                    source_path: &child_source_path,
                    target_path: &child_target_path,
                    source: &child_source,
                    target: &child_target,
                },
                options,
                actions,
            )?;
        } else if child_metadata.is_file() {
            plan_copy_file(
                operation,
                child_source,
                child_target,
                &child_source_path,
                &child_target_path,
                options,
                actions,
            )?;
        } else if child_metadata.is_dir() {
            plan_copy_directory(
                plan,
                operation,
                CopyEntry {
                    source_path: &child_source_path,
                    target_path: &child_target_path,
                    source: &child_source,
                    target: &child_target,
                },
                options,
                actions,
            )?;
        } else {
            return conflict(
                operation.operation,
                child_source_path,
                "source file type is unsupported",
            );
        }
    }

    Ok(())
}

fn plan_copy_file(
    operation: &PlannedFileOperation,
    source: PathBuf,
    target: PathBuf,
    source_path: &Path,
    target_path: &Path,
    options: FileApplyOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    match maybe_metadata(target_path, operation.operation)? {
        Some(metadata) if metadata.is_dir() => conflict(
            operation.operation,
            target_path.to_path_buf(),
            "target is a directory",
        ),
        Some(_) if options.strict => conflict(
            operation.operation,
            target_path.to_path_buf(),
            "target exists",
        ),
        Some(_) if options.force => {
            actions.push(FileAction::CopyFile {
                operation: operation.operation,
                source,
                target,
                source_path: source_path.to_path_buf(),
                target_path: target_path.to_path_buf(),
                replace: true,
            });
            Ok(())
        }
        Some(_) => {
            actions.push(FileAction::Skip {
                operation: operation.operation,
                target,
                reason: "target exists".to_owned(),
            });
            Ok(())
        }
        None => {
            actions.push(FileAction::CopyFile {
                operation: operation.operation,
                source,
                target,
                source_path: source_path.to_path_buf(),
                target_path: target_path.to_path_buf(),
                replace: false,
            });
            Ok(())
        }
    }
}

fn plan_copy_symlink(
    plan: &RunPlan,
    operation: &PlannedFileOperation,
    source_path: &Path,
    options: FileApplyOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    plan_copy_symlink_at(
        plan,
        operation,
        CopyEntry {
            source_path,
            target_path: &operation.target_path,
            source: &operation.source,
            target: &operation.target,
        },
        options,
        actions,
    )
}

fn plan_copy_symlink_at(
    plan: &RunPlan,
    operation: &PlannedFileOperation,
    entry: CopyEntry<'_>,
    options: FileApplyOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    let (link_target, final_target, target_is_dir) =
        preserved_source_link(plan, entry.source_path, entry.target_path)?;
    plan_symlink_action(
        SymlinkActionPlan {
            operation: operation.operation,
            source: entry.source.to_path_buf(),
            target: entry.target.to_path_buf(),
            target_path: entry.target_path.to_path_buf(),
            link_target,
            final_target,
            target_is_dir,
        },
        options,
        actions,
    )
}

fn plan_symlink(
    operation: &PlannedFileOperation,
    options: FileApplyOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    let target_parent = operation
        .target_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let link_target = relative_path(target_parent, &operation.source_path)
        .unwrap_or_else(|| operation.source_path.clone());

    plan_symlink_action(
        SymlinkActionPlan {
            operation: operation.operation,
            source: operation.source.clone(),
            target: operation.target.clone(),
            target_path: operation.target_path.clone(),
            link_target,
            final_target: operation.source_path.clone(),
            target_is_dir: operation.source_path.is_dir(),
        },
        options,
        actions,
    )
}

fn plan_symlink_action(
    plan: SymlinkActionPlan,
    options: FileApplyOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    match maybe_metadata(&plan.target_path, plan.operation)? {
        Some(metadata) if metadata.is_dir() => {
            conflict(plan.operation, plan.target_path, "target is a directory")
        }
        Some(_) if options.strict => conflict(plan.operation, plan.target_path, "target exists"),
        Some(_) if options.force => {
            actions.push(FileAction::CreateSymlink {
                operation: plan.operation,
                source: plan.source,
                target: plan.target,
                target_path: plan.target_path,
                link_target: plan.link_target,
                final_target: plan.final_target,
                target_is_dir: plan.target_is_dir,
                replace: true,
            });
            Ok(())
        }
        Some(_) => {
            actions.push(FileAction::Skip {
                operation: plan.operation,
                target: plan.target,
                reason: "target exists".to_owned(),
            });
            Ok(())
        }
        None => {
            actions.push(FileAction::CreateSymlink {
                operation: plan.operation,
                source: plan.source,
                target: plan.target,
                target_path: plan.target_path,
                link_target: plan.link_target,
                final_target: plan.final_target,
                target_is_dir: plan.target_is_dir,
                replace: false,
            });
            Ok(())
        }
    }
}

fn add_symlink_warnings(actions: &mut Vec<FileAction>) {
    let created_paths = actions
        .iter()
        .filter_map(|action| match action {
            FileAction::CreateDirectory { target_path, .. }
            | FileAction::CopyFile { target_path, .. }
            | FileAction::CreateSymlink { target_path, .. } => Some(target_path.clone()),
            FileAction::Skip { .. } | FileAction::Warning { .. } => None,
        })
        .collect::<BTreeSet<_>>();
    let warnings = actions
        .iter()
        .filter_map(|action| match action {
            FileAction::CreateSymlink {
                target,
                final_target,
                ..
            } if !final_target.exists() && !created_paths.contains(final_target) => {
                Some(FileAction::Warning {
                    path: target.clone(),
                    reason: "symlink target does not exist".to_owned(),
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    actions.extend(warnings);
}

fn report_dry_run(action: &FileAction, reporter: &mut dyn Reporter) -> Result<()> {
    match action {
        FileAction::CreateDirectory {
            operation,
            source,
            target,
            ..
        }
        | FileAction::CopyFile {
            operation,
            source,
            target,
            ..
        }
        | FileAction::CreateSymlink {
            operation,
            source,
            target,
            ..
        } => report(
            reporter,
            OutputEvent::FileWouldApply {
                operation: *operation,
                source: source.clone(),
                target: target.clone(),
            },
        ),
        FileAction::Skip {
            operation,
            target,
            reason,
        } => report(
            reporter,
            OutputEvent::FileWouldSkip {
                operation: *operation,
                target: target.clone(),
                reason: reason.clone(),
            },
        ),
        FileAction::Warning { path, reason } => report(
            reporter,
            OutputEvent::FileWarning {
                path: path.clone(),
                reason: reason.clone(),
            },
        ),
    }
}

fn apply_action(action: &FileAction, reporter: &mut dyn Reporter) -> Result<()> {
    match action {
        FileAction::CreateDirectory {
            operation,
            source,
            target,
            target_path,
        } => {
            fs::create_dir_all(target_path).map_err(|source| Error::FileOperationIo {
                operation: operation_name(*operation),
                path: target_path.clone(),
                source,
            })?;
            report_applied(reporter, *operation, source, target)
        }
        FileAction::CopyFile {
            operation,
            source,
            target,
            source_path,
            target_path,
            replace,
        } => {
            create_parent_dir(*operation, target_path)?;
            if *replace {
                remove_file(*operation, target_path)?;
            }
            copy_file_with_metadata(*operation, source_path, target_path)?;
            report_applied(reporter, *operation, source, target)
        }
        FileAction::CreateSymlink {
            operation,
            source,
            target,
            target_path,
            link_target,
            final_target: _,
            target_is_dir,
            replace,
        } => {
            create_parent_dir(*operation, target_path)?;
            if *replace {
                remove_file(*operation, target_path)?;
            }
            create_symlink(*operation, link_target, *target_is_dir, target_path)?;
            report_applied(reporter, *operation, source, target)
        }
        FileAction::Skip {
            operation,
            target,
            reason,
        } => report(
            reporter,
            OutputEvent::FileSkipped {
                operation: *operation,
                target: target.clone(),
                reason: reason.clone(),
            },
        ),
        FileAction::Warning { path, reason } => report(
            reporter,
            OutputEvent::FileWarning {
                path: path.clone(),
                reason: reason.clone(),
            },
        ),
    }
}

fn report_applied(
    reporter: &mut dyn Reporter,
    operation: FileOperationKind,
    source: &Path,
    target: &Path,
) -> Result<()> {
    report(
        reporter,
        OutputEvent::FileApplied {
            operation,
            source: source.to_path_buf(),
            target: target.to_path_buf(),
        },
    )
}

fn create_parent_dir(operation: FileOperationKind, target_path: &Path) -> Result<()> {
    let Some(parent) = target_path.parent() else {
        return Ok(());
    };

    fs::create_dir_all(parent).map_err(|source| Error::FileOperationIo {
        operation: operation_name(operation),
        path: parent.to_path_buf(),
        source,
    })
}

fn copy_file_with_metadata(
    operation: FileOperationKind,
    source_path: &Path,
    target_path: &Path,
) -> Result<()> {
    let metadata = metadata(source_path, operation)?;
    fs::copy(source_path, target_path).map_err(|source| Error::FileOperationIo {
        operation: operation_name(operation),
        path: target_path.to_path_buf(),
        source,
    })?;

    let mut times = FileTimes::new();
    if let Ok(accessed) = metadata.accessed() {
        times = times.set_accessed(accessed);
    }
    if let Ok(modified) = metadata.modified() {
        times = times.set_modified(modified);
    }
    File::options()
        .read(true)
        .open(target_path)
        .and_then(|file| file.set_times(times))
        .map_err(|source| Error::FileOperationIo {
            operation: operation_name(operation),
            path: target_path.to_path_buf(),
            source,
        })?;

    fs::set_permissions(target_path, metadata.permissions()).map_err(|source| {
        Error::FileOperationIo {
            operation: operation_name(operation),
            path: target_path.to_path_buf(),
            source,
        }
    })
}

fn remove_file(operation: FileOperationKind, path: &Path) -> Result<()> {
    fs::remove_file(path).map_err(|source| Error::FileOperationIo {
        operation: operation_name(operation),
        path: path.to_path_buf(),
        source,
    })
}

fn create_symlink(
    operation: FileOperationKind,
    source: &Path,
    target_is_dir: bool,
    target: &Path,
) -> Result<()> {
    create_symlink_impl(source, target, target_is_dir).map_err(|source| Error::FileOperationIo {
        operation: operation_name(operation),
        path: target.to_path_buf(),
        source,
    })
}

#[cfg(unix)]
fn create_symlink_impl(source: &Path, target: &Path, _target_is_dir: bool) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn create_symlink_impl(source: &Path, target: &Path, target_is_dir: bool) -> std::io::Result<()> {
    if target_is_dir {
        std::os::windows::fs::symlink_dir(source, target)
    } else {
        std::os::windows::fs::symlink_file(source, target)
    }
}

fn preserved_source_link(
    plan: &RunPlan,
    source_path: &Path,
    target_path: &Path,
) -> Result<(PathBuf, PathBuf, bool)> {
    let raw_target = fs::read_link(source_path).map_err(|source| Error::FileOperationIo {
        operation: operation_name(FileOperationKind::Copy),
        path: source_path.to_path_buf(),
        source,
    })?;
    if raw_target.as_os_str().is_empty() {
        return conflict(
            FileOperationKind::Copy,
            source_path.to_path_buf(),
            "source symlink target is empty",
        );
    }

    let source_parent = source_path.parent().unwrap_or_else(|| Path::new("."));
    let resolved_target = if raw_target.is_absolute() {
        raw_target.clone()
    } else {
        normalize_lexical(&source_parent.join(&raw_target))
    };
    let target_is_dir = fs::metadata(&resolved_target)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false);
    let final_target = resolved_target
        .strip_prefix(&plan.context.root_path)
        .map_or(resolved_target.clone(), |relative| {
            plan.context.worktree_path.join(relative)
        });
    let target_parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    let link_target =
        relative_path(target_parent, &final_target).unwrap_or_else(|| raw_target.clone());

    Ok((link_target, final_target, target_is_dir))
}

fn raw_source_path(plan: &RunPlan, operation: &PlannedFileOperation) -> PathBuf {
    if operation.source.is_absolute() {
        operation.source.clone()
    } else {
        normalize_lexical(&plan.context.root_path.join(&operation.source))
    }
}

fn metadata(path: &Path, operation: FileOperationKind) -> Result<Metadata> {
    fs::symlink_metadata(path).map_err(|source| Error::FileOperationIo {
        operation: operation_name(operation),
        path: path.to_path_buf(),
        source,
    })
}

fn maybe_metadata(path: &Path, operation: FileOperationKind) -> Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::FileOperationIo {
            operation: operation_name(operation),
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn conflict<T>(
    operation: FileOperationKind,
    path: PathBuf,
    message: impl Into<String>,
) -> Result<T> {
    Err(Error::FileOperationConflict {
        operation: operation_name(operation),
        path,
        message: message.into(),
    })
}

fn report(reporter: &mut dyn Reporter, event: OutputEvent) -> Result<()> {
    reporter
        .report(event)
        .map_err(|source| Error::Output { source })
}

fn operation_name(operation: FileOperationKind) -> &'static str {
    match operation {
        FileOperationKind::Copy => "copy",
        FileOperationKind::Symlink => "symlink",
        FileOperationKind::Sync => "sync",
    }
}

fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from_components = comparable_components(from)?;
    let to_components = comparable_components(to)?;

    if from_components.first() != to_components.first() {
        return None;
    }

    let common_len = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();

    for _ in &from_components[common_len..] {
        relative.push("..");
    }
    for component in &to_components[common_len..] {
        relative.push(component);
    }

    if relative.as_os_str().is_empty() {
        relative.push(".");
    }

    Some(relative)
}

fn comparable_components(path: &Path) -> Option<Vec<PathBuf>> {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => components.push(PathBuf::from(prefix.as_os_str())),
            Component::RootDir => components.push(PathBuf::from(component.as_os_str())),
            Component::Normal(part) => components.push(PathBuf::from(part)),
            Component::CurDir => {}
            Component::ParentDir => return None,
        }
    }

    Some(components)
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() && !normalized.has_root() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    normalized
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::{RunContext, SourceSpan};

    #[derive(Default)]
    struct VecReporter {
        events: Vec<OutputEvent>,
    }

    impl Reporter for VecReporter {
        fn report(&mut self, event: OutputEvent) -> std::io::Result<()> {
            self.events.push(event);
            Ok(())
        }
    }

    fn span() -> SourceSpan {
        SourceSpan {
            start: 0,
            end: 1,
            line: 1,
            column: 1,
        }
    }

    fn temp_workspace(name: &str) -> (PathBuf, PathBuf) {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after Unix epoch")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("treeboot-files-{name}-{id}"));
        let root = base.join("root");
        let worktree = base.join("worktree");

        fs::create_dir_all(&root).expect("root should be created");
        fs::create_dir_all(&worktree).expect("worktree should be created");

        (root, worktree)
    }

    fn context(root_path: &Path, worktree_path: &Path) -> RunContext {
        RunContext {
            root_path: root_path.to_path_buf(),
            worktree_path: worktree_path.to_path_buf(),
            default_branch: "main".to_owned(),
            environment: BTreeMap::from([(
                "TREEBOOT_ROOT_PATH".to_owned(),
                OsString::from(root_path),
            )]),
        }
    }

    fn operation(
        operation: FileOperationKind,
        root: &Path,
        worktree: &Path,
        source: &str,
        target: &str,
    ) -> PlannedFileOperation {
        PlannedFileOperation {
            operation,
            source: PathBuf::from(source),
            target: PathBuf::from(target),
            source_path: root.join(source),
            target_path: worktree.join(target),
            required: false,
            compare: None,
            delete: None,
            symlinks: None,
            status: PlannedFileStatus::Ready,
            declaration: span(),
        }
    }

    fn sync_operation(
        root: &Path,
        worktree: &Path,
        source: &str,
        target: &str,
    ) -> PlannedFileOperation {
        operation(FileOperationKind::Sync, root, worktree, source, target)
    }

    fn run_plan(root: &Path, worktree: &Path, files: Vec<PlannedFileOperation>) -> RunPlan {
        RunPlan {
            context: context(root, worktree),
            config_path: worktree.join(".treeboot.toml"),
            files,
            commands: Vec::new(),
        }
    }

    #[test]
    fn apply_file_operations_should_copy_missing_directory_files_only() {
        let (root, worktree) = temp_workspace("directory-copy");
        let source_dir = root.join("shared");
        let target_dir = worktree.join("shared");
        fs::create_dir_all(&source_dir).expect("source dir should be created");
        fs::create_dir_all(&target_dir).expect("target dir should be created");
        fs::write(source_dir.join("existing"), "new\n").expect("source should be written");
        fs::write(source_dir.join("missing"), "value\n").expect("source should be written");
        fs::write(target_dir.join("existing"), "old\n").expect("target should be written");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                "shared",
                "shared",
            )],
        );
        let mut reporter = VecReporter::default();

        apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
            .expect("directory copy should apply");

        let existing = fs::read_to_string(target_dir.join("existing"))
            .expect("existing target should be readable");
        let missing = fs::read_to_string(target_dir.join("missing"))
            .expect("missing target should be copied");
        assert_eq!(existing, "old\n");
        assert_eq!(missing, "value\n");
    }

    #[test]
    fn apply_file_operations_should_force_copy_directory_files_without_deleting_extras() {
        let (root, worktree) = temp_workspace("force-directory-copy");
        let source_dir = root.join("shared");
        let target_dir = worktree.join("shared");
        fs::create_dir_all(&source_dir).expect("source dir should be created");
        fs::create_dir_all(&target_dir).expect("target dir should be created");
        fs::write(source_dir.join("existing"), "new\n").expect("source should be written");
        fs::write(target_dir.join("existing"), "old\n").expect("target should be written");
        fs::write(target_dir.join("extra"), "keep\n").expect("extra target should be written");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                "shared",
                "shared",
            )],
        );
        let mut reporter = VecReporter::default();

        apply_file_operations(
            &plan,
            FileApplyOptions {
                force: true,
                ..FileApplyOptions::default()
            },
            &mut reporter,
        )
        .expect("forced directory copy should apply");

        let existing = fs::read_to_string(target_dir.join("existing"))
            .expect("existing target should be readable");
        let extra =
            fs::read_to_string(target_dir.join("extra")).expect("extra target should remain");
        assert_eq!(existing, "new\n");
        assert_eq!(extra, "keep\n");
    }

    #[test]
    fn apply_file_operations_should_reject_existing_directory_in_strict_before_children() {
        let (root, worktree) = temp_workspace("strict-directory-copy");
        let source_dir = root.join("shared");
        let target_dir = worktree.join("shared");
        fs::create_dir_all(&source_dir).expect("source dir should be created");
        fs::create_dir_all(&target_dir).expect("target dir should be created");
        fs::write(source_dir.join("missing"), "value\n").expect("source should be written");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                "shared",
                "shared",
            )],
        );
        let mut reporter = VecReporter::default();

        let error = apply_file_operations(
            &plan,
            FileApplyOptions {
                strict: true,
                ..FileApplyOptions::default()
            },
            &mut reporter,
        )
        .expect_err("strict directory conflict should fail");

        assert!(error.to_string().contains("target directory exists"));
        assert!(!target_dir.join("missing").exists());
    }

    #[test]
    fn apply_file_operations_should_reject_file_to_directory_target() {
        let (root, worktree) = temp_workspace("file-to-directory");
        fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
        fs::create_dir_all(worktree.join(".env")).expect("target dir should be created");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                ".env",
                ".env",
            )],
        );
        let mut reporter = VecReporter::default();

        let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
            .expect_err("file to directory should fail");

        assert!(error.to_string().contains("target is a directory"));
    }

    #[test]
    fn apply_file_operations_should_reject_directory_to_file_target() {
        let (root, worktree) = temp_workspace("directory-to-file");
        fs::create_dir_all(root.join("shared")).expect("source dir should be created");
        fs::write(worktree.join("shared"), "old\n").expect("target file should be written");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                "shared",
                "shared",
            )],
        );
        let mut reporter = VecReporter::default();

        let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
            .expect_err("directory to file should fail");

        assert!(error.to_string().contains("target is a file or symlink"));
    }

    #[test]
    fn apply_file_operations_should_skip_optional_missing_sources() {
        let (root, worktree) = temp_workspace("missing-source");
        let mut missing = operation(FileOperationKind::Copy, &root, &worktree, ".env", ".env");
        missing.status = PlannedFileStatus::SkippedMissingSource;
        let plan = run_plan(&root, &worktree, vec![missing]);
        let mut reporter = VecReporter::default();

        apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
            .expect("missing optional source should skip");

        assert!(matches!(
            reporter.events.as_slice(),
            [OutputEvent::FileSkipped { reason, .. }] if reason == "missing source"
        ));
    }

    #[test]
    fn apply_file_operations_should_gate_sync_before_copy_mutation() {
        let (root, worktree) = temp_workspace("sync-gate");
        fs::write(root.join(".env"), "TOKEN=1\n").expect("copy source should be written");
        fs::create_dir_all(root.join("shared")).expect("sync source should be created");
        let plan = run_plan(
            &root,
            &worktree,
            vec![
                operation(FileOperationKind::Copy, &root, &worktree, ".env", ".env"),
                sync_operation(&root, &worktree, "shared", "shared"),
            ],
        );
        let mut reporter = VecReporter::default();

        let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
            .expect_err("sync should be gated");

        assert!(error.to_string().contains("sync file operation execution"));
        assert!(!worktree.join(".env").exists());
    }

    #[test]
    fn apply_file_operations_should_leave_dry_run_unmutated() {
        let (root, worktree) = temp_workspace("dry-run");
        fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                ".env",
                ".env",
            )],
        );
        let mut reporter = VecReporter::default();

        apply_file_operations(
            &plan,
            FileApplyOptions {
                dry_run: true,
                ..FileApplyOptions::default()
            },
            &mut reporter,
        )
        .expect("dry-run should plan");

        assert!(!worktree.join(".env").exists());
        assert!(matches!(
            reporter.events.as_slice(),
            [OutputEvent::FileWouldApply { .. }]
        ));
    }

    #[cfg(unix)]
    #[test]
    fn apply_file_operations_should_copy_read_only_file() {
        use std::os::unix::fs::PermissionsExt;

        let (root, worktree) = temp_workspace("read-only-copy");
        let source = root.join(".env");
        let target = worktree.join(".env");
        fs::write(&source, "TOKEN=1\n").expect("source should be written");
        let mut permissions = fs::metadata(&source)
            .expect("source metadata should be readable")
            .permissions();
        permissions.set_mode(0o444);
        fs::set_permissions(&source, permissions).expect("source should become read-only");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                ".env",
                ".env",
            )],
        );
        let mut reporter = VecReporter::default();

        apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
            .expect("read-only source should copy");

        let copied = fs::read_to_string(&target).expect("target should be readable");
        let mode = fs::metadata(&target)
            .expect("target metadata should be readable")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(copied, "TOKEN=1\n");
        assert_eq!(mode, 0o444);
    }

    #[cfg(unix)]
    #[test]
    fn apply_file_operations_should_force_symlink_over_existing_file() {
        let (root, worktree) = temp_workspace("force-symlink");
        fs::write(root.join("tool"), "tool\n").expect("source should be written");
        fs::write(worktree.join(".tool"), "old\n").expect("target should be written");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Symlink,
                &root,
                &worktree,
                "tool",
                ".tool",
            )],
        );
        let mut reporter = VecReporter::default();

        apply_file_operations(
            &plan,
            FileApplyOptions {
                force: true,
                ..FileApplyOptions::default()
            },
            &mut reporter,
        )
        .expect("forced symlink should apply");

        let link = fs::read_link(worktree.join(".tool")).expect("target should be symlink");
        assert!(!link.is_absolute());
    }

    #[test]
    fn apply_file_operations_should_skip_existing_symlink_target_by_default() {
        let (root, worktree) = temp_workspace("skip-symlink");
        fs::write(root.join("tool"), "tool\n").expect("source should be written");
        fs::write(worktree.join(".tool"), "old\n").expect("target should be written");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Symlink,
                &root,
                &worktree,
                "tool",
                ".tool",
            )],
        );
        let mut reporter = VecReporter::default();

        apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
            .expect("existing target should skip");

        assert!(matches!(
            reporter.events.as_slice(),
            [OutputEvent::FileSkipped { reason, .. }] if reason == "target exists"
        ));
    }

    #[test]
    fn apply_file_operations_should_reject_symlink_to_existing_directory_even_with_force() {
        let (root, worktree) = temp_workspace("symlink-existing-dir");
        fs::write(root.join("tool"), "tool\n").expect("source should be written");
        fs::create_dir_all(worktree.join(".tool")).expect("target dir should be created");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Symlink,
                &root,
                &worktree,
                "tool",
                ".tool",
            )],
        );
        let mut reporter = VecReporter::default();

        let error = apply_file_operations(
            &plan,
            FileApplyOptions {
                force: true,
                ..FileApplyOptions::default()
            },
            &mut reporter,
        )
        .expect_err("symlink over directory should fail");

        assert!(error.to_string().contains("target is a directory"));
    }

    #[cfg(unix)]
    #[test]
    fn apply_file_operations_should_preserve_copied_source_symlinks() {
        let (root, worktree) = temp_workspace("preserved-symlink");
        let source_dir = root.join("shared");
        fs::create_dir_all(&source_dir).expect("source dir should be created");
        fs::write(source_dir.join("config"), "value\n").expect("source should be written");
        std::os::unix::fs::symlink("config", source_dir.join("link"))
            .expect("source symlink should be created");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                "shared",
                "shared",
            )],
        );
        let mut reporter = VecReporter::default();

        apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
            .expect("directory copy should apply");

        let link =
            fs::read_link(worktree.join("shared/link")).expect("copied symlink should exist");
        assert_eq!(link, PathBuf::from("config"));
    }

    #[cfg(unix)]
    #[test]
    fn apply_file_operations_should_warn_for_preserved_symlink_to_uncopied_target() {
        let (root, worktree) = temp_workspace("preserved-symlink-warning");
        let source_dir = root.join("shared");
        fs::create_dir_all(&source_dir).expect("source dir should be created");
        fs::write(source_dir.join("config"), "value\n").expect("source should be written");
        std::os::unix::fs::symlink("config", source_dir.join("link"))
            .expect("source symlink should be created");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                "shared/link",
                "shared/link",
            )],
        );
        let mut reporter = VecReporter::default();

        apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
            .expect("copied symlink should apply");

        assert!(reporter.events.iter().any(|event| matches!(
            event,
            OutputEvent::FileWarning { reason, .. }
                if reason == "symlink target does not exist"
        )));
    }

    #[cfg(unix)]
    #[test]
    fn preserved_source_link_should_track_directory_target_type() {
        let (root, worktree) = temp_workspace("preserved-directory-symlink");
        let source_dir = root.join("shared");
        fs::create_dir_all(source_dir.join("dir")).expect("source dir should be created");
        std::os::unix::fs::symlink("dir", source_dir.join("link"))
            .expect("source symlink should be created");
        let plan = run_plan(&root, &worktree, Vec::new());

        let (_, final_target, target_is_dir) = preserved_source_link(
            &plan,
            &source_dir.join("link"),
            &worktree.join("shared/link"),
        )
        .expect("preserved symlink should plan");

        assert_eq!(final_target, worktree.join("shared/dir"));
        assert!(target_is_dir);
    }
}

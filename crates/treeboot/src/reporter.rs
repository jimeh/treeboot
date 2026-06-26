use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use console::{Term, truncate_str};
use indicatif::{ProgressBar, ProgressStyle};
use treeboot_core::{FileOperationKind, OutputEvent, Reporter};

const DEFAULT_TERMINAL_WIDTH: usize = 80;
const PROGRESS_BAR_INDENT: &str = "          ";

pub(crate) struct StdoutReporter {
    active_progress: Option<ActiveProgress>,
    progress_enabled: bool,
}

struct ActiveProgress {
    bar: ProgressBar,
    label: Option<String>,
}

impl ActiveProgress {
    fn spinner(bar: ProgressBar) -> Self {
        Self { bar, label: None }
    }

    fn progress(bar: ProgressBar, label: String) -> Self {
        Self {
            bar,
            label: Some(label),
        }
    }
}

impl StdoutReporter {
    #[cfg(test)]
    fn with_progress_enabled(progress_enabled: bool) -> Self {
        Self {
            active_progress: None,
            progress_enabled,
        }
    }

    fn start_spinner(
        &mut self,
        operation: FileOperationKind,
        source: &std::path::Path,
        target: &std::path::Path,
    ) {
        if !self.progress_enabled {
            return;
        }
        if !matches!(operation, FileOperationKind::Copy | FileOperationKind::Sync) {
            return;
        }
        self.finish_progress();

        let bar = ProgressBar::new_spinner();
        if let Ok(style) = ProgressStyle::with_template("{msg} {spinner}") {
            bar.set_style(style);
        }
        bar.set_message(format!(
            "treeboot: {} {} -> {} planning",
            operation.as_str(),
            source.display(),
            target.display()
        ));
        bar.enable_steady_tick(Duration::from_millis(100));
        self.active_progress = Some(ActiveProgress::spinner(bar));
    }

    fn start_progress(
        &mut self,
        operation: FileOperationKind,
        source: &std::path::Path,
        target: &std::path::Path,
        action_count: usize,
    ) {
        self.finish_progress();
        if !self.progress_enabled {
            return;
        }
        if !matches!(operation, FileOperationKind::Copy | FileOperationKind::Sync) {
            return;
        }
        if action_count <= 1 {
            return;
        }

        let bar = ProgressBar::new(action_count as u64);
        let template = progress_bar_template();
        if let Ok(style) = ProgressStyle::with_template(&template) {
            bar.set_style(style.progress_chars("━╸─"));
        }
        let label = format!(
            "treeboot: {} {} -> {}",
            operation.as_str(),
            source.display(),
            target.display()
        );
        set_progress_message(&bar, &label);
        self.active_progress = Some(ActiveProgress::progress(bar, label));
    }

    fn advance_progress(&self) {
        if let Some(progress) = &self.active_progress {
            progress.bar.inc(1);
            if let Some(label) = &progress.label {
                set_progress_message(&progress.bar, label);
            }
        }
    }

    fn finish_progress(&mut self) {
        if let Some(progress) = self.active_progress.take() {
            progress.bar.finish_and_clear();
        }
    }

    fn print_line(&self, message: String) -> std::io::Result<()> {
        if message.is_empty() {
            return Ok(());
        }

        if let Some(progress) = &self.active_progress {
            progress.bar.suspend(|| write_line(&message))
        } else {
            write_line(&message)
        }
    }
}

impl Default for StdoutReporter {
    fn default() -> Self {
        Self {
            active_progress: None,
            progress_enabled: progress_enabled(
                io::stdout().is_terminal(),
                io::stderr().is_terminal(),
            ),
        }
    }
}

impl Drop for StdoutReporter {
    fn drop(&mut self) {
        self.finish_progress();
    }
}

impl Reporter for StdoutReporter {
    fn report(&mut self, event: OutputEvent) -> std::io::Result<()> {
        match &event {
            OutputEvent::FileOperationPlanningStarted {
                operation,
                source,
                target,
            } => self.start_spinner(*operation, source, target),
            OutputEvent::FileOperationPlanningFinished { .. } => self.finish_progress(),
            OutputEvent::FileOperationExecutionStarted {
                operation,
                source,
                target,
                action_count,
            } => self.start_progress(*operation, source, target, *action_count),
            OutputEvent::FileOperationActionAdvanced { .. } => self.advance_progress(),
            OutputEvent::FileOperationFinished { .. } => {
                self.finish_progress();
                self.print_line(event.message())?;
            }
            _ => self.print_line(event.message())?,
        }

        Ok(())
    }
}

fn set_progress_message(bar: &ProgressBar, label: &str) {
    let terminal_width = Term::stderr()
        .size_checked()
        .map_or(DEFAULT_TERMINAL_WIDTH, |size| usize::from(size.1));
    bar.set_message(progress_message(label, terminal_width));
}

fn progress_message(label: &str, terminal_width: usize) -> String {
    let tail = if terminal_width >= 4 { "..." } else { "" };
    truncate_str(label, terminal_width, tail).into_owned()
}

fn progress_bar_template() -> String {
    [
        "{msg}\n",
        PROGRESS_BAR_INDENT,
        "{bar:24.cyan/dim} {pos}/{len}",
    ]
    .concat()
}

const fn progress_enabled(stdout_is_terminal: bool, stderr_is_terminal: bool) -> bool {
    stdout_is_terminal && stderr_is_terminal
}

fn write_line(message: &str) -> std::io::Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{message}")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn source() -> PathBuf {
        PathBuf::from("shared")
    }

    fn target() -> PathBuf {
        PathBuf::from("local/shared")
    }

    fn active(reporter: &StdoutReporter) -> &ActiveProgress {
        reporter
            .active_progress
            .as_ref()
            .expect("progress should be active")
    }

    #[test]
    fn progress_message_should_keep_label_when_it_fits() {
        assert_eq!(
            progress_message("treeboot: sync shared -> shared", 80),
            "treeboot: sync shared -> shared"
        );
    }

    #[test]
    fn progress_message_should_truncate_to_terminal_width() {
        let message = progress_message(
            "treeboot: sync very/long/source/path -> very/long/target/path",
            32,
        );

        assert_eq!(console::measure_text_width(&message), 32);
        assert!(message.ends_with("..."));
    }

    #[test]
    fn progress_message_should_support_tiny_terminals() {
        assert_eq!(
            progress_message("treeboot: sync shared -> shared", 3),
            "tre"
        );
    }

    #[test]
    fn progress_bar_indent_should_align_after_treeboot_prefix() {
        assert_eq!(PROGRESS_BAR_INDENT.len(), "treeboot: ".len());
        assert!(progress_bar_template().starts_with("{msg}\n          "));
    }

    #[test]
    fn progress_enabled_should_require_stdout_and_stderr_terminals() {
        assert!(progress_enabled(true, true));
        assert!(!progress_enabled(false, true));
        assert!(!progress_enabled(true, false));
        assert!(!progress_enabled(false, false));
    }

    #[test]
    fn spinner_should_start_only_for_copy_and_sync_planning() {
        let mut reporter = StdoutReporter::with_progress_enabled(true);

        reporter.start_spinner(FileOperationKind::Symlink, &source(), &target());
        assert!(reporter.active_progress.is_none());

        reporter.start_spinner(FileOperationKind::Copy, &source(), &target());
        let progress = active(&reporter);
        assert_eq!(progress.label, None);
        assert_eq!(
            progress.bar.message(),
            "treeboot: copy shared -> local/shared planning"
        );

        reporter.start_spinner(FileOperationKind::Sync, &source(), &target());
        let progress = active(&reporter);
        assert_eq!(
            progress.bar.message(),
            "treeboot: sync shared -> local/shared planning"
        );
    }

    #[test]
    fn progress_should_not_start_for_single_actions_or_symlinks() {
        let mut reporter = StdoutReporter::with_progress_enabled(true);

        reporter.start_progress(FileOperationKind::Copy, &source(), &target(), 1);
        assert!(reporter.active_progress.is_none());

        reporter.start_progress(FileOperationKind::Sync, &source(), &target(), 0);
        assert!(reporter.active_progress.is_none());

        reporter.start_progress(FileOperationKind::Symlink, &source(), &target(), 3);
        assert!(reporter.active_progress.is_none());
    }

    #[test]
    fn progress_should_store_label_and_length_for_copy() {
        let mut reporter = StdoutReporter::with_progress_enabled(true);

        reporter.start_progress(FileOperationKind::Copy, &source(), &target(), 42);

        let progress = active(&reporter);
        assert_eq!(
            progress.label.as_deref(),
            Some("treeboot: copy shared -> local/shared")
        );
        assert_eq!(progress.bar.prefix(), "");
        assert_eq!(progress.bar.length(), Some(42));
        assert_eq!(progress.bar.position(), 0);
        assert_eq!(
            progress.bar.message(),
            "treeboot: copy shared -> local/shared"
        );
    }

    #[test]
    fn progress_should_store_sync_label_without_prefix() {
        let mut reporter = StdoutReporter::with_progress_enabled(true);

        reporter.start_progress(FileOperationKind::Sync, &source(), &target(), 12);

        let progress = active(&reporter);
        assert_eq!(progress.bar.prefix(), "");
        assert_eq!(
            progress.label.as_deref(),
            Some("treeboot: sync shared -> local/shared")
        );
    }

    #[test]
    fn progress_should_advance_position_and_keep_message() {
        let mut reporter = StdoutReporter::with_progress_enabled(true);

        reporter.start_progress(FileOperationKind::Sync, &source(), &target(), 12);
        reporter.advance_progress();
        reporter.advance_progress();

        let progress = active(&reporter);
        assert_eq!(progress.bar.position(), 2);
        assert_eq!(
            progress.bar.message(),
            "treeboot: sync shared -> local/shared"
        );
    }

    #[test]
    fn finish_progress_should_clear_active_progress() {
        let mut reporter = StdoutReporter::with_progress_enabled(true);

        reporter.start_progress(FileOperationKind::Copy, &source(), &target(), 2);
        assert!(reporter.active_progress.is_some());

        reporter.finish_progress();
        assert!(reporter.active_progress.is_none());
    }

    #[test]
    fn report_should_drive_progress_lifecycle() {
        let mut reporter = StdoutReporter::with_progress_enabled(true);
        let source = source();
        let target = target();

        reporter
            .report(OutputEvent::FileOperationExecutionStarted {
                operation: FileOperationKind::Sync,
                source: source.clone(),
                target: target.clone(),
                action_count: 3,
            })
            .expect("execution start should report");
        assert_eq!(active(&reporter).bar.position(), 0);

        reporter
            .report(OutputEvent::FileOperationActionAdvanced {
                operation: FileOperationKind::Sync,
                source,
                target,
            })
            .expect("action advanced should report");
        assert_eq!(active(&reporter).bar.position(), 1);

        reporter
            .report(OutputEvent::FileOperationPlanningFinished {
                operation: FileOperationKind::Sync,
                source: PathBuf::from("shared"),
                target: PathBuf::from("local/shared"),
                action_count: 3,
            })
            .expect("planning finished should report");
        assert!(reporter.active_progress.is_none());
    }

    #[test]
    fn report_should_suppress_progress_when_not_enabled() {
        let mut reporter = StdoutReporter::with_progress_enabled(false);

        reporter
            .report(OutputEvent::FileOperationPlanningStarted {
                operation: FileOperationKind::Copy,
                source: source(),
                target: target(),
            })
            .expect("planning start should report");
        assert!(reporter.active_progress.is_none());

        reporter
            .report(OutputEvent::FileOperationExecutionStarted {
                operation: FileOperationKind::Sync,
                source: source(),
                target: target(),
                action_count: 3,
            })
            .expect("execution start should report");
        assert!(reporter.active_progress.is_none());

        reporter
            .report(OutputEvent::FileOperationActionAdvanced {
                operation: FileOperationKind::Sync,
                source: source(),
                target: target(),
            })
            .expect("action advanced should report");
        assert!(reporter.active_progress.is_none());
    }
}

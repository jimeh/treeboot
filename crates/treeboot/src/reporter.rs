use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::time::Duration;

use console::{Term, truncate_str};
use indicatif::{ProgressBar, ProgressStyle};
use treeboot_core::{FileOperationKind, OutputEvent, Reporter};

const DEFAULT_TERMINAL_WIDTH: usize = 80;
const PROGRESS_BAR_TEMPLATE: &str = "{msg}\n          {bar:24.cyan/dim} {pos}/{len}";

pub(crate) struct StdoutReporter {
    active_progress: Option<ActiveProgress>,
    progress_enabled: bool,
    #[cfg(test)]
    line_writer: fn(&str) -> std::io::Result<()>,
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
            line_writer: write_line,
        }
    }

    #[cfg(test)]
    fn with_line_writer(
        progress_enabled: bool,
        line_writer: fn(&str) -> std::io::Result<()>,
    ) -> Self {
        Self {
            active_progress: None,
            progress_enabled,
            line_writer,
        }
    }

    fn start_spinner(&mut self, operation: FileOperationKind, source: &Path, target: &Path) {
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
        source: &Path,
        target: &Path,
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
        if let Ok(style) = ProgressStyle::with_template(PROGRESS_BAR_TEMPLATE) {
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
            progress.bar.suspend(|| self.write_message(&message))
        } else {
            self.write_message(&message)
        }
    }

    fn write_message(&self, message: &str) -> std::io::Result<()> {
        #[cfg(test)]
        {
            (self.line_writer)(message)
        }
        #[cfg(not(test))]
        {
            write_line(message)
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
            #[cfg(test)]
            line_writer: write_line,
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
        match event {
            OutputEvent::FileOperationPlanningStarted {
                operation,
                source,
                target,
            } => {
                self.start_spinner(operation, &source, &target);
                Ok(())
            }
            OutputEvent::FileOperationPlanningFinished { .. } => {
                self.finish_progress();
                Ok(())
            }
            OutputEvent::FileOperationExecutionStarted {
                operation,
                source,
                target,
                action_count,
            } => {
                self.start_progress(operation, &source, &target, action_count);
                Ok(())
            }
            OutputEvent::FileOperationActionAdvanced { .. } => {
                self.advance_progress();
                Ok(())
            }
            OutputEvent::FileOperationFinished {
                operation,
                source,
                target,
                summary,
                dry_run,
            } => {
                self.finish_progress();
                self.print_line(summary.message(operation, &source, &target, dry_run))
            }
            other => self.print_line(other.message()),
        }
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
    use std::io::Write as _;
    use std::path::PathBuf;

    use super::*;
    use treeboot_core::FileOperationSummary;

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

    fn sink_line(message: &str) -> std::io::Result<()> {
        std::io::sink().write_all(message.as_bytes())
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
    fn progress_message_should_omit_ellipsis_when_it_cannot_fit() {
        assert_eq!(progress_message("treeboot: sync shared -> shared", 0), "");
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
        let (_, bar_line) = PROGRESS_BAR_TEMPLATE
            .split_once('\n')
            .expect("progress template should contain message and bar lines");
        let indent_width = bar_line
            .find("{bar")
            .expect("progress template should include a bar token");

        assert_eq!(indent_width, "treeboot: ".len());
        assert_eq!(
            PROGRESS_BAR_TEMPLATE,
            "{msg}\n          {bar:24.cyan/dim} {pos}/{len}"
        );
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
    fn single_action_progress_should_clear_existing_spinner() {
        let mut reporter = StdoutReporter::with_progress_enabled(true);

        reporter.start_spinner(FileOperationKind::Copy, &source(), &target());
        assert!(reporter.active_progress.is_some());

        reporter.start_progress(FileOperationKind::Copy, &source(), &target(), 1);
        assert!(reporter.active_progress.is_none());
    }

    #[test]
    fn multi_action_progress_should_replace_existing_spinner() {
        let mut reporter = StdoutReporter::with_progress_enabled(true);

        reporter.start_spinner(FileOperationKind::Sync, &source(), &target());
        assert_eq!(active(&reporter).label, None);

        reporter.start_progress(FileOperationKind::Sync, &source(), &target(), 2);
        let progress = active(&reporter);
        assert_eq!(
            progress.label.as_deref(),
            Some("treeboot: sync shared -> local/shared")
        );
        assert_eq!(progress.bar.length(), Some(2));
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
    fn print_line_should_ignore_empty_messages() {
        let reporter = StdoutReporter::with_progress_enabled(true);

        reporter
            .print_line(String::new())
            .expect("empty messages should be ignored");
    }

    #[test]
    fn print_line_should_suspend_active_progress() {
        let mut reporter = StdoutReporter::with_line_writer(true, sink_line);

        reporter.start_progress(FileOperationKind::Copy, &source(), &target(), 2);
        reporter
            .print_line("treeboot: warning: example".to_owned())
            .expect("line should print while progress is active");

        assert!(reporter.active_progress.is_some());
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
                source: source.clone(),
                target: target.clone(),
            })
            .expect("action advanced should report");
        assert_eq!(active(&reporter).bar.position(), 1);

        reporter
            .report(OutputEvent::FileOperationFinished {
                operation: FileOperationKind::Sync,
                source,
                target,
                summary: FileOperationSummary {
                    changed: 1,
                    expanded: true,
                    ..FileOperationSummary::default()
                },
                dry_run: false,
            })
            .expect("execution finished should report");
        assert!(reporter.active_progress.is_none());
    }

    #[test]
    fn report_should_suppress_progress_when_not_enabled() {
        let mut reporter = StdoutReporter::with_progress_enabled(false);
        let source = source();
        let target = target();

        reporter
            .report(OutputEvent::FileOperationPlanningStarted {
                operation: FileOperationKind::Copy,
                source: source.clone(),
                target: target.clone(),
            })
            .expect("planning start should report");
        assert!(reporter.active_progress.is_none());

        reporter
            .report(OutputEvent::FileOperationExecutionStarted {
                operation: FileOperationKind::Sync,
                source: source.clone(),
                target: target.clone(),
                action_count: 3,
            })
            .expect("execution start should report");
        assert!(reporter.active_progress.is_none());

        reporter
            .report(OutputEvent::FileOperationActionAdvanced {
                operation: FileOperationKind::Sync,
                source,
                target,
            })
            .expect("action advanced should report");
        assert!(reporter.active_progress.is_none());
    }
}

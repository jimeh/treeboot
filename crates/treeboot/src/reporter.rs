use std::time::Duration;

use console::{Term, measure_text_width, truncate_str};
use indicatif::{ProgressBar, ProgressStyle};
use treeboot_core::{FileOperationKind, OutputEvent, Reporter};

const MIN_PROGRESS_BAR_WIDTH: usize = 24;
const DEFAULT_TERMINAL_WIDTH: usize = 80;

#[derive(Default)]
pub(crate) struct StdoutReporter {
    active_progress: Option<ActiveProgress>,
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
    fn start_spinner(
        &mut self,
        operation: FileOperationKind,
        source: &std::path::Path,
        target: &std::path::Path,
    ) {
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
        if !matches!(operation, FileOperationKind::Copy | FileOperationKind::Sync) {
            return;
        }
        if action_count <= 1 {
            return;
        }

        let bar = ProgressBar::new(action_count as u64);
        let template = "{msg} {wide_bar:.cyan/blue}";
        if let Ok(style) = ProgressStyle::with_template(template) {
            bar.set_style(style.progress_chars("=> "));
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

    fn print_line(&self, message: String) {
        if message.is_empty() {
            return;
        }

        if let Some(progress) = &self.active_progress {
            progress.bar.suspend(|| println!("{message}"));
        } else {
            println!("{message}");
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
                self.print_line(event.message());
            }
            _ => self.print_line(event.message()),
        }

        Ok(())
    }
}

fn set_progress_message(bar: &ProgressBar, label: &str) {
    let count = format_progress_count(bar);
    let terminal_width = Term::stderr()
        .size_checked()
        .map_or(DEFAULT_TERMINAL_WIDTH, |size| usize::from(size.1));
    bar.set_message(progress_message(label, &count, terminal_width));
}

fn format_progress_count(bar: &ProgressBar) -> String {
    let position = bar.position();
    match bar.length() {
        Some(length) => format!("{position}/{length}"),
        None => position.to_string(),
    }
}

fn progress_message(label: &str, count: &str, terminal_width: usize) -> String {
    let reserved_width = measure_text_width(count) + MIN_PROGRESS_BAR_WIDTH + 2;
    let label_width = terminal_width.saturating_sub(reserved_width);
    if label_width == 0 {
        return count.to_owned();
    }

    let tail = if label_width >= 4 { "..." } else { "" };
    format!("{} {count}", truncate_str(label, label_width, tail))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_message_should_keep_count_next_to_label() {
        assert_eq!(
            progress_message("treeboot: sync shared -> shared", "12/40", 80),
            "treeboot: sync shared -> shared 12/40"
        );
    }

    #[test]
    fn progress_message_should_reserve_minimum_bar_width() {
        let message = progress_message(
            "treeboot: sync very/long/source/path -> very/long/target/path",
            "12/40",
            48,
        );

        assert_eq!(measure_text_width(&message), 23);
        assert!(message.ends_with(" 12/40"));
    }

    #[test]
    fn progress_message_should_prefer_count_on_tiny_terminals() {
        assert_eq!(
            progress_message("treeboot: sync shared -> shared", "12/40", 20),
            "12/40"
        );
    }
}

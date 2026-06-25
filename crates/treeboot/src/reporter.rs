use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use treeboot_core::{FileOperationKind, OutputEvent, Reporter};

#[derive(Default)]
pub(crate) struct StdoutReporter {
    active_progress: Option<ProgressBar>,
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
        if let Ok(style) = ProgressStyle::with_template("{spinner} {msg}") {
            bar.set_style(style);
        }
        bar.set_message(format!(
            "treeboot: {} {} -> {} planning",
            operation.as_str(),
            source.display(),
            target.display()
        ));
        bar.enable_steady_tick(Duration::from_millis(100));
        self.active_progress = Some(bar);
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
        if let Ok(style) = ProgressStyle::with_template("{bar:40.cyan/blue} {pos}/{len} {msg}") {
            bar.set_style(style.progress_chars("=> "));
        }
        bar.set_message(format!(
            "treeboot: {} {} -> {}",
            operation.as_str(),
            source.display(),
            target.display()
        ));
        self.active_progress = Some(bar);
    }

    fn advance_progress(&self) {
        if let Some(progress) = &self.active_progress {
            progress.inc(1);
        }
    }

    fn finish_progress(&mut self) {
        if let Some(progress) = self.active_progress.take() {
            progress.finish_and_clear();
        }
    }

    fn print_line(&self, message: String) {
        if message.is_empty() {
            return;
        }

        if let Some(progress) = &self.active_progress {
            progress.suspend(|| println!("{message}"));
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

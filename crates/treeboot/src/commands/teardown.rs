use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use clap::Args;
use treeboot_core::{
    Reporter, TeardownExecuteOptions, TeardownOptions, execute_teardown, prepare_teardown,
};

use super::{CliError, environment_input};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct TeardownArgs {
    /// Select the linked worktree to tear down.
    #[arg(long)]
    worktree: Option<PathBuf>,

    /// Override the root checkout used for discovery.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Use one specific config file instead of config discovery.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Report teardown commands without prompting or running them.
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Approve teardown commands without an interactive prompt.
    #[arg(long)]
    yes: bool,
}

pub(crate) fn run_teardown_command(
    args: TeardownArgs,
    reporter: &mut dyn Reporter,
) -> Result<(), CliError> {
    let prepared = prepare_teardown(
        TeardownOptions {
            cwd: args.worktree,
            root: args.root,
            environment: environment_input(),
            config: args.config,
        },
        reporter,
    )?;
    let Some(plan) = prepared.plan() else {
        return Ok(());
    };

    if !args.dry_run && !args.yes {
        let stdin = io::stdin();
        if !stdin.is_terminal() {
            return Err(CliError::ConfirmationRequired);
        }
        let mut input = stdin.lock();
        let stderr = io::stderr();
        let mut output = stderr.lock();
        if !confirm(
            &mut input,
            &mut output,
            prepared.context().worktree_path.as_path(),
            plan.commands().len(),
        )? {
            return Err(CliError::TeardownDeclined);
        }
    }

    execute_teardown(
        plan,
        TeardownExecuteOptions {
            dry_run: args.dry_run,
        },
        reporter,
    )?;
    Ok(())
}

fn confirm(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    worktree: &std::path::Path,
    command_count: usize,
) -> Result<bool, CliError> {
    writeln!(
        output,
        "Run {command_count} teardown commands for {}?",
        worktree.display()
    )
    .map_err(CliError::PromptIo)?;
    write!(
        output,
        "These commands may delete resources outside the worktree. [y/N] "
    )
    .map_err(CliError::PromptIo)?;
    output.flush().map_err(CliError::PromptIo)?;

    let mut answer = String::new();
    input.read_line(&mut answer).map_err(CliError::PromptIo)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingIo;

    impl io::Read for FailingIo {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("read failed"))
        }
    }

    impl io::BufRead for FailingIo {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            Err(io::Error::other("read failed"))
        }

        fn consume(&mut self, _amount: usize) {}
    }

    impl io::Write for FailingIo {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("write failed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush failed"))
        }
    }

    #[test]
    fn confirmation_accepts_only_yes_values() {
        for value in ["y\n", "YES\n", " yes "] {
            let mut input = io::Cursor::new(value.as_bytes());
            let mut output = Vec::new();
            assert!(
                confirm(
                    &mut input,
                    &mut output,
                    std::path::Path::new("/worktree"),
                    2
                )
                .expect("prompt should succeed")
            );
        }

        for value in ["", "\n", "n\n", "anything\n"] {
            let mut input = io::Cursor::new(value.as_bytes());
            let mut output = Vec::new();
            assert!(
                !confirm(
                    &mut input,
                    &mut output,
                    std::path::Path::new("/worktree"),
                    2
                )
                .expect("prompt should succeed")
            );
        }
    }

    #[test]
    fn confirmation_maps_prompt_io_failures() {
        let mut input = io::Cursor::new(b"yes\n");
        let error = confirm(
            &mut input,
            &mut FailingIo,
            std::path::Path::new("/worktree"),
            1,
        )
        .expect_err("write failure should be returned");
        assert!(matches!(error, CliError::PromptIo(_)));

        let mut output = Vec::new();
        let error = confirm(
            &mut FailingIo,
            &mut output,
            std::path::Path::new("/worktree"),
            1,
        )
        .expect_err("read failure should be returned");
        assert!(matches!(error, CliError::PromptIo(_)));
    }
}

use std::path::PathBuf;

use clap::Args;
use treeboot_core::RunOptions;

use super::environment_input;

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct RunArgs {
    /// Override the checkout used as the file-operation source.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Use one specific config file instead of config discovery.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Fail on missing config and stricter file-operation conflicts.
    #[arg(short = 'S', long)]
    strict: bool,

    /// Replace existing file-operation targets where supported.
    #[arg(short, long)]
    force: bool,

    /// Print planned work without changing files or running commands.
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Print detailed file-operation actions.
    #[arg(short, long)]
    verbose: bool,

    /// Run file operations only.
    #[arg(long)]
    skip_commands: bool,
}

impl From<RunArgs> for RunOptions {
    fn from(args: RunArgs) -> Self {
        Self {
            cwd: None,
            root: args.root,
            environment: environment_input(),
            config: args.config,
            strict: args.strict,
            force: args.force,
            dry_run: args.dry_run,
            verbose: args.verbose,
            skip_commands: args.skip_commands,
        }
    }
}

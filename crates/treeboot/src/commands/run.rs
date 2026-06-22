use std::path::PathBuf;

use clap::Args;
use treeboot_core::RunOptions;

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct RunArgs {
    /// Override the checkout used as the file-operation source.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Use one specific config file and skip init script discovery.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Skip init script discovery and use declarative config discovery.
    #[arg(long)]
    no_init_script: bool,

    /// Fail on missing config and stricter file-operation conflicts.
    #[arg(short = 'S', long)]
    strict: bool,

    /// Replace existing file-operation targets where supported.
    #[arg(short, long)]
    force: bool,

    /// Print planned work without changing files or running commands.
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Run file operations only.
    #[arg(long)]
    skip_commands: bool,
}

impl From<RunArgs> for RunOptions {
    fn from(args: RunArgs) -> Self {
        Self {
            cwd: None,
            root: args.root,
            config: args.config,
            no_init_script: args.no_init_script,
            strict: args.strict,
            force: args.force,
            dry_run: args.dry_run,
            skip_commands: args.skip_commands,
        }
    }
}

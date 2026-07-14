use std::path::PathBuf;

use clap::Args;
use treeboot_core::InitOptions;

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct InitArgs {
    /// Create a starter TOML config.
    #[arg(long = "config")]
    _config: bool,

    /// Output path for the generated file.
    #[arg(short, long)]
    path: Option<PathBuf>,
}

impl From<InitArgs> for InitOptions {
    fn from(args: InitArgs) -> Self {
        Self {
            cwd: None,
            path: args.path,
        }
    }
}

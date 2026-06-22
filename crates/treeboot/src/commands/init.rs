use std::path::PathBuf;

use clap::Args;
use treeboot_core::{InitKind, InitOptions};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct InitArgs {
    /// Create a starter TOML config.
    #[arg(long, conflicts_with = "script")]
    config: bool,

    /// Create an executable init script.
    #[arg(short, long, conflicts_with = "config")]
    script: bool,

    /// Output path for the generated file.
    #[arg(short, long)]
    path: Option<PathBuf>,
}

impl From<InitArgs> for InitOptions {
    fn from(args: InitArgs) -> Self {
        Self {
            cwd: None,
            kind: init_kind(&args),
            path: args.path,
        }
    }
}

fn init_kind(args: &InitArgs) -> InitKind {
    match (args.config, args.script) {
        (false, true) => InitKind::Script,
        _ => InitKind::Config,
    }
}

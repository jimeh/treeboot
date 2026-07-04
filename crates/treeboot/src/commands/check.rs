use std::path::PathBuf;

use clap::Args;
use treeboot_core::CheckOptions;

use super::environment_input;
use super::output::{OutputArgs, ReportFormat, write_structured};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct CheckArgs {
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

    #[command(flatten)]
    output: OutputArgs,
}

pub(crate) fn run_check_command(args: CheckArgs) -> treeboot_core::Result<()> {
    let format = args.output.format();
    let report = treeboot_core::check(args.into())?;

    match format {
        ReportFormat::Text => {
            for warning in &report.warnings {
                println!("treeboot: warning: {warning}");
            }
            println!("treeboot: check ok");
            Ok(())
        }
        format => write_structured(&report, format),
    }
}

impl From<CheckArgs> for CheckOptions {
    fn from(args: CheckArgs) -> Self {
        Self {
            cwd: None,
            root: args.root,
            environment: environment_input(),
            config: args.config,
            no_init_script: args.no_init_script,
            strict: args.strict,
        }
    }
}

use std::path::PathBuf;

use clap::Args;
use treeboot_core::EnvOptions;

use super::environment_input;
use super::output::{OutputArgs, ReportFormat, write_structured};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct EnvArgs {
    /// Override the checkout used for environment discovery.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Use one specific config file and skip config discovery.
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[command(flatten)]
    output: OutputArgs,
}

pub(crate) fn run_env_command(args: EnvArgs) -> treeboot_core::Result<()> {
    let format = args.output.format();
    let report = treeboot_core::inspect_env(args.into())?;

    match format {
        ReportFormat::Text => {
            for (name, value) in report.environment {
                println!("{name}={value}");
            }
            Ok(())
        }
        format => write_structured(&report, format),
    }
}

impl From<EnvArgs> for EnvOptions {
    fn from(args: EnvArgs) -> Self {
        let mut options = Self::default();
        options.root = args.root;
        options.environment = environment_input();
        options.config = args.config;
        options
    }
}

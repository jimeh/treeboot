use std::path::PathBuf;

use clap::Args;
use treeboot_core::{PlanOptions, Reporter};

use super::environment_input;
use super::output::{
    BootstrapReportMode, OutputArgs, ReportFormat, SilentReporter, write_bootstrap_report,
};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct PlanArgs {
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

    /// Print detailed file-operation actions.
    #[arg(short, long)]
    verbose: bool,

    /// Omit configured commands from the plan.
    #[arg(long)]
    skip_commands: bool,

    #[command(flatten)]
    output: OutputArgs,
}

impl PlanArgs {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.output.format() != ReportFormat::Text && self.verbose {
            Err("--verbose cannot be used with JSON or YAML output")
        } else {
            Ok(())
        }
    }
}

pub(crate) fn run_plan_command(
    args: PlanArgs,
    reporter: &mut dyn Reporter,
) -> treeboot_core::Result<()> {
    let format = args.output.format();
    let options = args.into();
    match format {
        ReportFormat::Text => treeboot_core::plan(options, reporter).map(|_| ()),
        format => {
            let mut silent = SilentReporter;
            let report = treeboot_core::plan(options, &mut silent)?;
            write_bootstrap_report(&report, BootstrapReportMode::Plan, format)
        }
    }
}

impl From<PlanArgs> for PlanOptions {
    fn from(args: PlanArgs) -> Self {
        let mut options = Self::default();
        options.root = args.root;
        options.environment = environment_input();
        options.config = args.config;
        options.strict = args.strict;
        options.force = args.force;
        options.verbose = args.verbose;
        options.skip_commands = args.skip_commands;
        options
    }
}

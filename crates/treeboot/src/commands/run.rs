use std::path::PathBuf;

use clap::Args;
use treeboot_core::{Reporter, RunOptions};

use super::environment_input;
use super::output::{
    BootstrapReportMode, OutputArgs, ReportFormat, SilentReporter, write_bootstrap_report,
};

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

    #[command(flatten)]
    output: OutputArgs,
}

impl RunArgs {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let format = self.output.format();
        if format != ReportFormat::Text && self.verbose {
            return Err("--verbose cannot be used with JSON or YAML output");
        }
        if format != ReportFormat::Text && !self.dry_run && !self.skip_commands {
            return Err("structured run output requires --dry-run or --skip-commands");
        }
        Ok(())
    }

    pub(crate) fn output_is_specified(&self) -> bool {
        self.output.is_specified()
    }
}

pub(crate) fn run_run_command(
    args: RunArgs,
    reporter: &mut dyn Reporter,
) -> treeboot_core::Result<()> {
    let format = args.output.format();
    let dry_run = args.dry_run;
    let options = args.into();
    match format {
        ReportFormat::Text => treeboot_core::run(options, reporter).map(|_| ()),
        format => {
            let mut silent = SilentReporter;
            let report = treeboot_core::run_detailed(options, &mut silent)?;
            let mode = if dry_run {
                BootstrapReportMode::DryRun
            } else {
                BootstrapReportMode::Execute
            };
            write_bootstrap_report(&report, mode, format)
        }
    }
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

use std::path::PathBuf;

use clap::Args;
use treeboot_core::{DiagnosticStatus, DoctorOptions, Error};

use super::environment_input;
use super::output::{OutputArgs, ReportFormat, write_structured};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct DoctorArgs {
    /// Override the checkout used for diagnostics.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Use one specific config file and skip init script discovery.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Skip init script discovery and use declarative config discovery.
    #[arg(long)]
    no_init_script: bool,

    #[command(flatten)]
    output: OutputArgs,
}

pub(crate) fn run_doctor_command(args: DoctorArgs) -> treeboot_core::Result<()> {
    let format = args.output.format();
    let report = treeboot_core::diagnose(args.into());

    match format {
        ReportFormat::Text => print_doctor_text(&report),
        format => write_structured(&report, format)?,
    }

    if report.has_fatal() {
        Err(Error::DoctorFailed)
    } else {
        Ok(())
    }
}

fn print_doctor_text(report: &treeboot_core::DoctorReport) {
    println!("treeboot: doctor");
    for diagnostic in &report.diagnostics {
        let status = match diagnostic.status {
            DiagnosticStatus::Ok => "ok",
            DiagnosticStatus::Warning => "warning",
            DiagnosticStatus::Error => "error",
        };
        println!("{status}: {}: {}", diagnostic.name, diagnostic.message);
    }
}

impl From<DoctorArgs> for DoctorOptions {
    fn from(args: DoctorArgs) -> Self {
        Self {
            cwd: None,
            root: args.root,
            environment: environment_input(),
            config: args.config,
            no_init_script: args.no_init_script,
        }
    }
}

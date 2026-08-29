use std::io::Write;

use clap::{Args, ValueEnum};
use serde::Serialize;
use treeboot_core::{BootstrapReport, Error, OutputEvent, Reporter};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct OutputArgs {
    /// Output format.
    #[arg(short = 'o', long, value_enum, conflicts_with_all = ["json", "yaml"])]
    format: Option<ReportFormat>,

    /// Print output as JSON.
    #[arg(short = 'J', long, conflicts_with_all = ["format", "yaml"])]
    json: bool,

    /// Print output as YAML.
    #[arg(short = 'Y', long, conflicts_with_all = ["format", "json"])]
    yaml: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum ReportFormat {
    #[default]
    Text,
    Json,
    Yaml,
}

impl OutputArgs {
    pub(crate) fn format(&self) -> ReportFormat {
        if self.json {
            ReportFormat::Json
        } else if self.yaml {
            ReportFormat::Yaml
        } else {
            self.format.unwrap_or_default()
        }
    }

    pub(crate) fn is_specified(&self) -> bool {
        self.format.is_some() || self.json || self.yaml
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BootstrapReportMode {
    Plan,
    DryRun,
    Execute,
}

#[derive(Serialize)]
struct CliBootstrapReport<'a> {
    mode: BootstrapReportMode,
    #[serde(flatten)]
    report: &'a BootstrapReport,
}

pub(crate) fn write_bootstrap_report(
    report: &BootstrapReport,
    mode: BootstrapReportMode,
    format: ReportFormat,
) -> treeboot_core::Result<()> {
    write_structured(&CliBootstrapReport { mode, report }, format)
}

#[derive(Default)]
pub(crate) struct SilentReporter;

impl Reporter for SilentReporter {
    fn report(&mut self, _event: OutputEvent) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn write_structured<T>(value: &T, format: ReportFormat) -> treeboot_core::Result<()>
where
    T: Serialize,
{
    let mut output = Vec::new();
    match format {
        ReportFormat::Text => {
            return Err(Error::Output {
                source: std::io::Error::other("text output is handled by each command"),
            });
        }
        ReportFormat::Json => {
            serde_json::to_writer_pretty(&mut output, value).map_err(|source| Error::Output {
                source: std::io::Error::other(source),
            })?;
            output.push(b'\n');
        }
        ReportFormat::Yaml => {
            yaml_serde::to_writer(&mut output, value).map_err(|source| Error::Output {
                source: std::io::Error::other(source),
            })?;
        }
    }

    std::io::stdout()
        .lock()
        .write_all(&output)
        .map_err(|source| Error::Output { source })
}

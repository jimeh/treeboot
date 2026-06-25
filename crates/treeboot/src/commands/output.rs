use std::io::Write;

use clap::{Args, ValueEnum};
use serde::Serialize;
use treeboot_core::Error;

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
}

pub(crate) fn write_structured<T>(value: &T, format: ReportFormat) -> treeboot_core::Result<()>
where
    T: Serialize,
{
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    match format {
        ReportFormat::Text => {
            return Err(Error::Output {
                source: std::io::Error::other("text output is handled by each command"),
            });
        }
        ReportFormat::Json => {
            serde_json::to_writer_pretty(&mut handle, value).map_err(|source| Error::Output {
                source: std::io::Error::other(source),
            })?;
            writeln!(handle).map_err(|source| Error::Output { source })?;
        }
        ReportFormat::Yaml => {
            yaml_serde::to_writer(&mut handle, value).map_err(|source| Error::Output {
                source: std::io::Error::other(source),
            })?;
        }
    }

    Ok(())
}

use std::path::PathBuf;

use clap::Args;
use treeboot_core::{
    CommandKind, CommandOperation, ConfigOptions, ConfigReport, Error, FileOperation,
    RuntimeOptionOverrides,
};

use super::output::{OutputArgs, ReportFormat, write_structured};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct ConfigArgs {
    /// Override the checkout used for config discovery.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Use one specific config file and skip config discovery.
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[command(flatten)]
    output: OutputArgs,
}

pub(crate) fn run_config_command(args: ConfigArgs) -> treeboot_core::Result<()> {
    let format = args.output.format();
    let env_options = RuntimeOptionOverrides::from_env()?;
    let report = treeboot_core::inspect_config(args.into())?;

    match format {
        ReportFormat::Text => print_config_text(&report).map_err(|source| Error::Output { source }),
        format => {
            let output = serde_json::json!({
                "path": report.path,
                "config": report.config,
            });
            write_structured(&output, format)
        }
    }?;

    let plan_options = env_options.resolve(&report.config.options, false);

    if let Err(error) = treeboot_core::ActionPlan::from_manifest(
        &report.path,
        &report.config,
        &report.context,
        plan_options.into(),
    ) {
        eprintln!("treeboot: warning: run validation would fail: {error}");
    }

    Ok(())
}

fn print_config_text(report: &ConfigReport) -> std::io::Result<()> {
    println!("treeboot: config {}", report.path.display());
    println!();
    println!("files:");
    if report.config.files.is_empty() {
        println!("  (none)");
    } else {
        for operation in &report.config.files {
            println!("  {}", file_operation_summary(operation));
        }
    }
    println!();
    println!("commands:");
    if report.config.commands.is_empty() {
        println!("  (none)");
    } else {
        for command in &report.config.commands {
            println!("  {}", command_summary(command));
        }
    }

    Ok(())
}

fn file_operation_summary(operation: &FileOperation) -> String {
    let mut summary = format!(
        "{} {} -> {}",
        operation.operation.as_str(),
        operation.source.display(),
        operation.target.display()
    );

    if operation.required {
        summary.push_str(" required=true");
    }
    if let Some(compare) = operation.compare {
        summary.push_str(&format!(" compare={compare:?}").to_lowercase());
    }
    if let Some(delete) = operation.delete {
        summary.push_str(&format!(" delete={delete}"));
    }
    if let Some(symlinks) = operation.symlinks {
        summary.push_str(&format!(" symlinks={symlinks:?}").to_lowercase());
    }

    summary
}

fn command_summary(command: &CommandOperation) -> String {
    let mut summary = match &command.command {
        CommandKind::Shell { run } => format!("run {run:?}"),
        CommandKind::Direct { program, args } => {
            let mut parts = vec![program.as_str()];
            parts.extend(args.iter().map(String::as_str));
            format!("exec {}", parts.join(" "))
        }
    };

    if command.allow_failure {
        summary.push_str(" allow_failure=true");
    }
    if let Some(cwd) = &command.cwd {
        summary.push_str(&format!(" cwd={}", cwd.display()));
    }
    if !command.env.is_empty() {
        let env = command
            .env
            .iter()
            .map(|(name, value)| format!("{name}={value:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        summary.push_str(&format!(" env={{{env}}}"));
    }

    summary
}

impl From<ConfigArgs> for ConfigOptions {
    fn from(args: ConfigArgs) -> Self {
        Self {
            cwd: None,
            root: args.root,
            config: args.config,
        }
    }
}

use std::path::PathBuf;

use clap::Args;
use treeboot_core::{
    CommandKind, CommandOperation, ConfigOptions, ConfigReport, Environment, Error, FileOperation,
    RuntimePolicy,
};

use super::environment_input;
use super::output::{OutputArgs, ReportFormat, write_structured};

const WORKTREE_ID_ENV: &str = "TREEBOOT_WORKTREE_ID";
const WORKTREE_SLUG_ENV: &str = "TREEBOOT_WORKTREE_SLUG";

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
    let options: ConfigOptions = args.into();
    // `treeboot config` has no CLI strict flag; it only uses config/env policy
    // to warn whether run validation would fail.
    let runtime_policy = RuntimePolicy::from_environment(&options.environment, false)?;
    let report = treeboot_core::inspect_config(options)?;
    let worktree_id = effective_identity_value(&report.context.environment, WORKTREE_ID_ENV)
        .ok_or_else(|| Error::Output {
            source: std::io::Error::other(format!(
                "resolved command environment is missing {WORKTREE_ID_ENV}"
            )),
        })?;
    let worktree_slug = effective_identity_value(&report.context.environment, WORKTREE_SLUG_ENV)
        .ok_or_else(|| Error::Output {
            source: std::io::Error::other(format!(
                "resolved command environment is missing {WORKTREE_SLUG_ENV}"
            )),
        })?;

    match format {
        ReportFormat::Text => print_config_text(&report, &worktree_id, &worktree_slug)
            .map_err(|source| Error::Output { source }),
        format => {
            let output = serde_json::json!({
                "path": report.path,
                "worktree_id": worktree_id,
                "worktree_slug": worktree_slug,
                "config": report.config,
            });
            write_structured(&output, format)
        }
    }?;

    let plan_options = runtime_policy.resolve(&report.config.options);

    // Warnings go to stderr in every output format so JSON and YAML stdout
    // output stays parseable.
    let phases = treeboot_core::validate_config_phases(
        &report.path,
        &report.config,
        &report.context,
        plan_options.into_action_plan_options(),
    );
    match phases.bootstrap() {
        Ok(plan) => {
            for warning in plan.warnings() {
                eprintln!("treeboot: warning: {warning}");
            }
        }
        Err(error) => {
            eprintln!("treeboot: warning: bootstrap validation would fail: {error}");
        }
    }
    if let Err(error) = phases.teardown() {
        eprintln!("treeboot: warning: teardown validation would fail: {error}");
    }

    Ok(())
}

fn print_config_text(
    report: &ConfigReport,
    worktree_id: &str,
    worktree_slug: &str,
) -> std::io::Result<()> {
    println!("treeboot: config {}", report.path.display());
    println!();
    println!("worktree id:");
    println!("  value: {worktree_id}");
    println!("  length: {}", report.config.worktree_id.length());
    println!();
    println!("worktree slug:");
    println!("  value: {worktree_slug}");
    println!("  max_length: {}", report.config.worktree_slug.max_length());
    println!(
        "  separator: {:?}",
        report.config.worktree_slug.separator().to_string()
    );
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
    println!();
    println!("teardown commands:");
    if report.config.teardown_commands.is_empty() {
        println!("  (none)");
    } else {
        for command in &report.config.teardown_commands {
            println!("  {}", command_summary(command));
        }
    }

    Ok(())
}

fn effective_identity_value(environment: &Environment, name: &str) -> Option<String> {
    environment
        .get(name)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string_lossy().into_owned())
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
    if !operation.include.is_empty() {
        let included = operation
            .include
            .iter()
            .map(|pattern| format!("{pattern:?}"))
            .collect::<Vec<_>>()
            .join(",");
        summary.push_str(&format!(" include=[{included}]"));
    }
    if !operation.ignore.is_empty() {
        let ignored = operation
            .ignore
            .iter()
            .map(|pattern| format!("{pattern:?}"))
            .collect::<Vec<_>>()
            .join(",");
        summary.push_str(&format!(" ignore=[{ignored}]"));
    }
    if !operation.ignore_metadata.is_empty() {
        let ignored = operation
            .ignore_metadata
            .iter()
            .map(|field| format!("{field:?}").to_lowercase())
            .collect::<Vec<_>>()
            .join(",");
        summary.push_str(&format!(" ignore_metadata=[{ignored}]"));
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
            environment: environment_input(),
            config: args.config,
        }
    }
}

#[cfg(test)]
mod tests {
    use treeboot_core::Environment;

    use super::effective_identity_value;

    #[test]
    fn effective_worktree_id_should_require_a_non_empty_managed_value() {
        let mut environment = Environment::new();
        assert_eq!(
            effective_identity_value(&environment, "TREEBOOT_WORKTREE_ID"),
            None
        );

        environment.insert("TREEBOOT_WORKTREE_ID".to_owned(), "".into());
        assert_eq!(
            effective_identity_value(&environment, "TREEBOOT_WORKTREE_ID"),
            None
        );

        environment.insert("TREEBOOT_WORKTREE_ID".to_owned(), "linked-a1b2c3".into());
        assert_eq!(
            effective_identity_value(&environment, "TREEBOOT_WORKTREE_ID").as_deref(),
            Some("linked-a1b2c3")
        );
    }
}

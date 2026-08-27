use std::ffi::OsString;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use treeboot_spec::{
    CONFIG_SCHEMA_JSON, CaseOutcome, CommandTemplate, RunOptions, SPEC_MARKDOWN, Suite,
};

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run conformance cases against a candidate command.
    Test {
        /// Report format written to stdout.
        #[arg(long, value_enum, default_value_t = ReportFormat::Human)]
        format: ReportFormat,
        /// Run only cases whose stable identifier contains this text.
        #[arg(long)]
        filter: Option<String>,
        /// Timeout for each candidate invocation.
        #[arg(long, default_value_t = 30_000)]
        timeout_ms: u64,
        /// Candidate program followed by optional arguments placed before every invocation.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        candidate: Vec<OsString>,
    },
    /// List stable conformance case identifiers.
    List,
    /// Print the canonical Treeboot specification Markdown.
    Show,
    /// Print the canonical Treeboot configuration JSON Schema.
    Schema,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReportFormat {
    Human,
    Json,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("treeboot-spec: {error}");
            ExitCode::from(3)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    match cli.command {
        Command::List => {
            for case in Suite::current().cases() {
                println!("{}", case.id());
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Show => {
            print!("{SPEC_MARKDOWN}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Schema => {
            print!("{CONFIG_SCHEMA_JSON}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Test {
            format,
            filter,
            timeout_ms,
            mut candidate,
        } => {
            let program = candidate.remove(0);
            let template = CommandTemplate::with_args(program, candidate);
            let report = Suite::current().run(
                &template,
                RunOptions {
                    filter,
                    invocation_timeout: Duration::from_millis(timeout_ms),
                },
            )?;
            if report.cases.is_empty() {
                eprintln!("treeboot-spec: no conformance cases match the requested filter");
                return Ok(ExitCode::from(2));
            }
            match format {
                ReportFormat::Human => print_human_report(&report),
                ReportFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
            }

            if report.passed() {
                return Ok(ExitCode::SUCCESS);
            }
            if report.cases.iter().any(|case| {
                matches!(
                    case.outcome,
                    CaseOutcome::Error { .. } | CaseOutcome::TimedOut { .. }
                )
            }) {
                return Ok(ExitCode::from(3));
            }
            Ok(ExitCode::from(1))
        }
    }
}

fn print_human_report(report: &treeboot_spec::SuiteReport) {
    println!(
        "treeboot-spec {} (spec {}, {})",
        report.crate_version, report.specification_version, report.host_platform
    );
    println!("candidate: {}", display_candidate(&report.candidate));
    for result in &report.cases {
        let case = format!(
            "{} [spec: {}] ({} ms)",
            result.case.id(),
            result.case.spec_references().join(", "),
            result.duration_ms
        );
        match &result.outcome {
            CaseOutcome::Passed => println!("PASS {case}"),
            CaseOutcome::Skipped { reason } => {
                println!("SKIP {case}: {reason}");
            }
            CaseOutcome::Failed { details } => {
                println!("FAIL {case}: {details}");
            }
            CaseOutcome::Error { details } => {
                println!("ERROR {case}: {details}");
            }
            CaseOutcome::TimedOut { details } => {
                println!("TIMEOUT {case}: {details}");
            }
        }
    }
    println!(
        "{} passed, {} skipped, {} failed",
        report.passed_count(),
        report.skipped_count(),
        report.cases.len() - report.passed_count() - report.skipped_count()
    );
}

fn display_candidate(candidate: &treeboot_spec::CommandReport) -> String {
    std::iter::once(candidate.program.as_str())
        .chain(candidate.prefix_args.iter().map(String::as_str))
        .map(display_argument)
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_argument(argument: &str) -> String {
    if !argument.is_empty()
        && argument
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "/._-:=+".contains(character))
    {
        argument.to_owned()
    } else {
        serde_json::to_string(argument).unwrap_or_else(|_| format!("{argument:?}"))
    }
}

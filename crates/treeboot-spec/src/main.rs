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
    println!("candidate: {}", report.candidate.program);
    for result in &report.cases {
        match &result.outcome {
            CaseOutcome::Passed => println!("PASS {}", result.case.id()),
            CaseOutcome::Skipped { reason } => {
                println!("SKIP {}: {reason}", result.case.id());
            }
            CaseOutcome::Failed { details } => {
                println!("FAIL {}: {details}", result.case.id());
            }
            CaseOutcome::Error { details } => {
                println!("ERROR {}: {details}", result.case.id());
            }
            CaseOutcome::TimedOut { details } => {
                println!("TIMEOUT {}: {details}", result.case.id());
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

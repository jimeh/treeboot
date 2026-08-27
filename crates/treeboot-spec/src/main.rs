use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use treeboot_spec::{
    CONFIG_SCHEMA_JSON, CaseOutcome, CommandTemplate, ConformanceProfile, Invocation,
    LocalProcessRunner, RunOptions, Runner, SPEC_MARKDOWN, Suite, SuiteEvent, Termination,
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
        /// Compatibility profile to run.
        #[arg(long, value_enum, default_value_t = Profile::Full)]
        profile: Profile,
        /// Run only cases whose stable identifier contains this text.
        #[arg(long)]
        filter: Option<String>,
        /// Timeout for each candidate invocation.
        #[arg(long, default_value_t = 30_000)]
        timeout_ms: u64,
        /// List every case result in human output.
        #[arg(long)]
        verbose: bool,
        /// Disable terminal progress output.
        #[arg(long)]
        no_progress: bool,
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Profile {
    Full,
    Functional,
}

impl From<Profile> for ConformanceProfile {
    fn from(profile: Profile) -> Self {
        match profile {
            Profile::Full => Self::Full,
            Profile::Functional => Self::Functional,
        }
    }
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
            profile,
            filter,
            timeout_ms,
            verbose,
            no_progress,
            mut candidate,
        } => {
            let program = candidate.remove(0);
            let template = CommandTemplate::with_args(program, candidate);
            let profile = profile.into();
            let invocation_timeout = Duration::from_millis(timeout_ms);
            let progress_enabled =
                matches!(format, ReportFormat::Human) && !no_progress && io::stderr().is_terminal();
            let report = {
                let mut stderr = io::stderr().lock();
                let mut progress = ProgressRenderer::new(&mut stderr);
                let report = Suite::current().run_observed(
                    &template,
                    RunOptions {
                        profile,
                        filter,
                        invocation_timeout,
                    },
                    |event| {
                        if progress_enabled {
                            progress.observe(event);
                        }
                    },
                )?;
                if progress_enabled {
                    progress.clear();
                }
                report
            };
            if report.cases.is_empty() {
                eprintln!("treeboot-spec: no conformance cases match the requested filter");
                return Ok(ExitCode::from(2));
            }
            match format {
                ReportFormat::Human => {
                    let candidate_metadata =
                        probe_candidate_metadata(&template, invocation_timeout);
                    print_human_report(&report, candidate_metadata.as_ref(), verbose);
                }
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

#[derive(Debug)]
struct CandidateMetadata {
    package: String,
    version: String,
    specification_version: String,
}

fn probe_candidate_metadata(
    command: &CommandTemplate,
    invocation_timeout: Duration,
) -> Option<CandidateMetadata> {
    const MAX_METADATA_BYTES: usize = 64 * 1024;
    let runner = LocalProcessRunner::new(command.resolve().ok()?);
    let result = runner
        .run(
            &Invocation::new()
                .args(["version", "--json"])
                .timeout(invocation_timeout.min(Duration::from_secs(2))),
        )
        .ok()?;
    if result.termination() != (Termination::Exited { code: 0 })
        || result.stdout().len() > MAX_METADATA_BYTES
    {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(result.stdout()).ok()?;
    Some(CandidateMetadata {
        package: value.get("package")?.as_str()?.to_owned(),
        version: value.get("version")?.as_str()?.to_owned(),
        specification_version: value.get("spec_version")?.as_str()?.to_owned(),
    })
}

fn print_human_report(
    report: &treeboot_spec::SuiteReport,
    candidate_metadata: Option<&CandidateMetadata>,
    verbose: bool,
) {
    println!(
        "treeboot-spec {} (spec {}, {})",
        report.crate_version, report.specification_version, report.host_platform
    );
    println!("candidate: {}", display_candidate(&report.candidate));
    println!("profile: {}", profile_name(report.profile));
    if let Some(metadata) = candidate_metadata {
        println!(
            "candidate version: {} {} (spec {})",
            metadata.package, metadata.version, metadata.specification_version
        );
        if metadata.specification_version != report.specification_version {
            match report.profile {
                ConformanceProfile::Full => println!(
                    "warning: candidate declares spec {}; full conformance requires spec {}",
                    metadata.specification_version, report.specification_version
                ),
                ConformanceProfile::Functional => println!(
                    "note: candidate spec {} differs from suite spec {}; functional compatibility allows this mismatch",
                    metadata.specification_version, report.specification_version
                ),
            }
        }
    }
    if report.omitted_exact_case_count > 0 {
        let case_suffix = if report.omitted_exact_case_count == 1 {
            ""
        } else {
            "s"
        };
        println!(
            "note: {} exact-identity case{case_suffix} omitted by the functional profile",
            report.omitted_exact_case_count,
        );
    }

    if verbose {
        println!("\nCases:");
        for result in &report.cases {
            println!(
                "  {} {} [spec: {}] ({} ms)",
                outcome_label(&result.outcome),
                result.case.id(),
                result.case.spec_references().join(", "),
                result.duration_ms
            );
        }
    }

    let failed_count = report
        .cases
        .iter()
        .filter(|result| matches!(result.outcome, CaseOutcome::Failed { .. }))
        .count();
    let error_count = report
        .cases
        .iter()
        .filter(|result| matches!(result.outcome, CaseOutcome::Error { .. }))
        .count();
    let timed_out_count = report
        .cases
        .iter()
        .filter(|result| matches!(result.outcome, CaseOutcome::TimedOut { .. }))
        .count();
    println!("\nResult: {}", result_label(report));
    let case_suffix = if report.cases.len() == 1 { "" } else { "s" };
    println!(
        "  {} case{case_suffix}: {} passed, {} skipped, {} failed, {} errors, {} timed out",
        report.cases.len(),
        report.passed_count(),
        report.skipped_count(),
        failed_count,
        error_count,
        timed_out_count
    );

    let skipped = report
        .cases
        .iter()
        .filter(|result| result.outcome.is_skipped())
        .collect::<Vec<_>>();
    if !skipped.is_empty() {
        println!("\nSkipped:");
        for result in skipped {
            let CaseOutcome::Skipped { reason } = &result.outcome else {
                continue;
            };
            println!("  {}: {reason}", result.case.id());
        }
    }

    let failures = report
        .cases
        .iter()
        .filter(|result| !result.outcome.is_passed() && !result.outcome.is_skipped())
        .collect::<Vec<_>>();
    if failures.is_empty() {
        return;
    }

    println!("\nFailures:");
    for result in &failures {
        println!("  {} {}", outcome_label(&result.outcome), result.case.id());
    }

    println!("\nFailure details:");
    for (index, result) in failures.into_iter().enumerate() {
        println!(
            "\n{}. {} [{}]",
            index + 1,
            result.case.id(),
            outcome_label(&result.outcome)
        );
        println!("   spec: {}", result.case.spec_references().join(", "));
        println!("   duration: {} ms", result.duration_ms);
        if let Some(details) = outcome_details(&result.outcome) {
            for line in details.lines() {
                println!("   {line}");
            }
        }
    }
}

fn profile_name(profile: ConformanceProfile) -> &'static str {
    match profile {
        ConformanceProfile::Full => "full",
        ConformanceProfile::Functional => "functional",
    }
}

fn result_label(report: &treeboot_spec::SuiteReport) -> &'static str {
    if report.passed() {
        return match report.profile {
            ConformanceProfile::Full => "PASSED",
            ConformanceProfile::Functional => "FUNCTIONALLY COMPATIBLE",
        };
    }
    if report.cases.iter().any(|result| {
        matches!(
            result.outcome,
            CaseOutcome::Error { .. } | CaseOutcome::TimedOut { .. }
        )
    }) {
        "ERROR"
    } else {
        "FAILED"
    }
}

fn outcome_label(outcome: &CaseOutcome) -> &'static str {
    match outcome {
        CaseOutcome::Passed => "PASS",
        CaseOutcome::Skipped { .. } => "SKIP",
        CaseOutcome::Failed { .. } => "FAIL",
        CaseOutcome::Error { .. } => "ERROR",
        CaseOutcome::TimedOut { .. } => "TIMEOUT",
    }
}

fn outcome_details(outcome: &CaseOutcome) -> Option<&str> {
    match outcome {
        CaseOutcome::Failed { details }
        | CaseOutcome::Error { details }
        | CaseOutcome::TimedOut { details } => Some(details),
        CaseOutcome::Passed | CaseOutcome::Skipped { .. } => None,
    }
}

struct ProgressRenderer<W> {
    writer: W,
    completed: usize,
    passed: usize,
    skipped: usize,
    failed: usize,
    errors: usize,
    timed_out: usize,
    previous_width: usize,
}

impl<W: Write> ProgressRenderer<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            completed: 0,
            passed: 0,
            skipped: 0,
            failed: 0,
            errors: 0,
            timed_out: 0,
            previous_width: 0,
        }
    }

    fn observe(&mut self, event: SuiteEvent<'_>) {
        match event {
            SuiteEvent::SuiteStarted { .. } => {}
            SuiteEvent::CaseStarted { total, case, .. } => self.render(total, case.id()),
            SuiteEvent::CaseFinished { total, result, .. } => {
                self.completed += 1;
                match result.outcome {
                    CaseOutcome::Passed => self.passed += 1,
                    CaseOutcome::Skipped { .. } => self.skipped += 1,
                    CaseOutcome::Failed { .. } => self.failed += 1,
                    CaseOutcome::Error { .. } => self.errors += 1,
                    CaseOutcome::TimedOut { .. } => self.timed_out += 1,
                }
                self.render(total, result.case.id());
            }
            _ => {}
        }
    }

    fn render(&mut self, total: usize, current_case: &str) {
        let percent = self
            .completed
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(100);
        let line = format!(
            "[{}/{}] {}%  {} passed  {} skipped  {} failed  {} errors  {} timed out  {}",
            self.completed,
            total,
            percent,
            self.passed,
            self.skipped,
            self.failed,
            self.errors,
            self.timed_out,
            current_case
        );
        let padding = " ".repeat(self.previous_width.saturating_sub(line.len()));
        let _ = write!(self.writer, "\r{line}{padding}");
        let _ = self.writer.flush();
        self.previous_width = line.len();
    }

    fn clear(&mut self) {
        let _ = write!(self.writer, "\r{}\r", " ".repeat(self.previous_width));
        let _ = self.writer.flush();
        self.previous_width = 0;
    }
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

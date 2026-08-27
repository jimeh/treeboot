use std::time::Duration;

use treeboot_spec::{CommandTemplate, ConformanceProfile, RunOptions, Suite};

#[test]
fn official_binary_should_pass_full_conformance_suite() {
    let command = CommandTemplate::new(env!("CARGO_BIN_EXE_treeboot"));
    let report = Suite::current()
        .run(
            &command,
            RunOptions {
                profile: ConformanceProfile::Full,
                invocation_timeout: Duration::from_secs(10),
                ..RunOptions::default()
            },
        )
        .expect("conformance suite should start");
    assert_eq!(report.profile(), ConformanceProfile::Full);
    assert_eq!(report.cases.len(), 322);
    assert_eq!(report.omitted_exact_case_count(), 0);

    let failures = report
        .cases
        .iter()
        .filter(|result| !result.outcome.is_passed() && !result.outcome.is_skipped())
        .map(|result| format!("{}: {:?}", result.case.id(), result.outcome))
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "official binary conformance failures:\n{}",
        failures.join("\n")
    );
}

# treeboot-spec

`treeboot-spec` is the executable, language-agnostic compatibility contract for
Treeboot implementations. It packages the canonical [specification](SPEC.md),
config [JSON Schema](assets/treeboot.schema.json), and black-box conformance
cases.

The suite invokes a candidate executable directly. Build the candidate once and
pass its path rather than using `cargo run` for each of the hundreds of case
invocations.

```console
treeboot-spec test -- /path/to/treeboot
treeboot-spec test --profile functional -- /path/to/treeboot
treeboot-spec test --format json -- /path/to/treeboot
treeboot-spec test --filter run.sync -- /path/to/treeboot
treeboot-spec list
treeboot-spec show
treeboot-spec schema
```

The default `full` profile verifies functional behavior plus the candidate's
declared specification version and exact canonical schema bytes. The
`functional` profile checks portable behavior while allowing those identities to
differ. A functional-profile pass covers only the selected cases and preserves
explicit capability skips for the caller to assess. It does not establish full
conformance with this crate's specification release.

Human output is concise by default. It prints a summary, skips, failing case
identifiers, then numbered failure details. Add `--verbose` to list passing
cases too. When stderr is a terminal, the CLI displays one live progress line;
`--no-progress` disables it. JSON reports never write progress to stderr and
retain the same serialized fields across profiles.

Rust callers use the same registry:

```rust
use treeboot_spec::{CommandTemplate, ConformanceProfile, RunOptions, Suite};

let implementation = CommandTemplate::new("/path/to/treeboot");
let report = Suite::current().run(
    &implementation,
    RunOptions {
        profile: ConformanceProfile::Full,
        ..RunOptions::default()
    },
)?;
assert!(report.passed());
# Ok::<(), treeboot_spec::SuiteError>(())
```

Use `Suite::run_observed` or `Suite::run_with_observer` to receive synchronous
suite-start, case-start, and case-finish events. Observers can render progress
or collect telemetry without coupling the suite to a terminal.

Custom `Runner` implementations can execute the same cases in another process
environment. A complete adapter must expose each temporary fixture filesystem to
the candidate and honor native arguments, working directories, environment
changes, stdin mode, deadlines, and per-stream output capture limits. A runner
must continue draining output after a limit is reached, then return
`RunnerError::OutputLimitExceeded`.

The default local runner captures output through pipes, so terminal-input cases
report an explicit capability skip. A custom adapter can provide terminal input
for those cases. Installed completion-script cases likewise run only when the
adapter reports that generated scripts can reinvoke its candidate from the
fixture host.

`treeboot-spec` does not specify Treeboot's Rust library API and has no
dependency on the official `treeboot` or `treeboot-core` crates.

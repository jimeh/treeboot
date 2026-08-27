# Validation Guide

Use this guide to pick the smallest useful feedback loop for a change.

## Tiers

### Targeted

Use while iterating on a narrow change:

```sh
mise run format
mise run format:check
mise run format:markdown
mise run lint:markdown
mise run test:core
mise run test:cli
mise run test:spec
mise run test:conformance
mise run test:release-helper
```

Use `test:core` for library behavior, `test:spec` for conformance-crate
self-tests, `test:conformance` for the official binary's portable contract, and
`test:cli` for adapter unit tests plus retained reference-only integration tests
and the official conformance driver. Use `test:release-helper` for release
workflow helper logic. Running `mise run test` executes the same packages
together. The aggregate and core test tasks also run doctests so compile-fail
public API contracts stay enforced. `format` applies Rust and Markdown
formatting, while `format:check` is non-mutating.

The official and standalone conformance drivers request the `full` profile
explicitly. For compatibility checks against another released implementation,
run `treeboot-spec test --profile functional -- <candidate>`. A green functional
run permits a different declared spec version and canonical schema; it does not
replace the full release gate.

Run `mise run audit:deps` after dependency changes to check `Cargo.lock` against
the current RustSec advisory database.

During a correction loop, classify the delta as production behavior, public
contract/API coverage, tests or CI fixtures, or documentation. Run the affected
targeted tasks and any mutation evidence required by the changed behavior.
Preserve prior broad validation when the correction cannot invalidate it; do not
restart `check` or `verify` after every narrow correction.

### Check

Use once on the intended handoff head for most code changes:

```sh
mise run check
```

This runs formatting checks, generated-artifact freshness checks, clippy,
Markdown linting, and repo harness invariants, then tests.

### Verify

Use once on the intended final local head for broad, CI-facing, release-facing,
or harness changes:

```sh
mise run verify
```

This runs the local CI task set plus coverage. Coverage is not a required merge
gate; it is a sensor for finding untested behavior.

## CI Mapping

GitHub Actions runs these mise tasks:

- `mise run actions:lint`
- `mise run audit:deps`
- `mise run format:check`
- `mise run generate:check`
  - currently wraps `mise run generate:schema:check`
- `mise run harness:check`
- `mise run lint`
- `mise run msrv`
- `mise run test`
- `mise run test:spec:standalone`

The full test suite runs once on each supported GitHub Actions host platform:
Linux x64/ARM64, macOS x64/ARM64, and Windows x64/ARM64. The local `mise run ci`
task mirrors the task set, but only on the current host platform.

## Cross-platform Preflight

For filesystem, path, CLI-output, or platform-gated test changes:

- Audit `cfg` gates and fixture assumptions against every supported CI host.
- Do not assume a filesystem fixture constructible on Linux is constructible on
  macOS or Windows.
- Make path assertions match the production rendering contract, including the
  choice between `Display`, `Debug`, and structured serialization and the
  resulting Windows escaping.
- When a filesystem fixture is impossible on one platform, gate that fixture
  narrowly and retain platform-independent coverage of the underlying behavior.
- Treat local `mise run ci` as evidence for the current host only; use the final
  GitHub Actions matrix for cross-platform confirmation.

## Coverage

For quick coverage feedback:

```sh
mise run coverage
```

The coverage tasks install `cargo-llvm-cov` through task-scoped mise tooling
instead of the top-level tool set used by every CI job.

The current suite is intentionally strongest around config discovery and
execution, legacy init-script non-execution, config-only init output,
environment propagation, and output formatting.

Useful follow-up coverage areas:

- declarative validation before side effects
- file-operation validation before side effects
- command runtime sequencing and failure behavior
- sync conflict and explicit delete behavior

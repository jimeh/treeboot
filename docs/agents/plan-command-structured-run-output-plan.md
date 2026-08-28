# Plan command and structured run output plan

Status: approved.

## Decision

Add a side-effect-free `treeboot plan` command and structured JSON/YAML output
for `treeboot run` modes that cannot spawn configured commands.

The primary integration is:

```sh
treeboot plan --skip-commands --json
```

An editor can read `has_file_changes` and prompt only when Treeboot would
create, replace, update, repair metadata on, or delete a target. Skipped
operations and warnings do not count as file changes.

`treeboot run` accepts structured output when either `--dry-run` or
`--skip-commands` prevents configured command output from contaminating the
structured document:

```sh
treeboot run --dry-run --json
treeboot run --skip-commands --json
treeboot run --dry-run --skip-commands --yaml
```

`treeboot run --json` remains a usage error. Normal configured commands inherit
stdout and stderr. Redirecting or capturing arbitrary child output would change
their behavior and is outside this work.

## Outcome

After this change:

- people have an explicitly safe planning command instead of relying on
  execution-shaped `run --dry-run` syntax;
- extensions and other tools receive one stable report rather than parsing
  human-readable lifecycle lines;
- `plan`, structured dry-run, text dry-run, and real execution use the same
  filesystem decision engine;
- a confirmed file-only run can return a structured report of what it applied,
  including non-fatal warnings produced during execution;
- `treeboot check` keeps its current meaning: validate bootstrap and teardown
  plans without answering whether the current filesystem differs.

The observable behavior must be added to
[`crates/treeboot-spec/SPEC.md`](../../crates/treeboot-spec/SPEC.md) before the
implementation is complete. The new subcommand and newly accepted structured run
forms require a visible spec-version bump from `2.5.1` to `2.6.0`, with the
README reference and generated spec-version asset kept in sync.

## Current constraints

The implementation already has most of the needed information, but it exposes
that information through text-oriented execution events:

- `treeboot check` parses, normalizes, and validates bootstrap and teardown
  phases. It does not compare targets with the current filesystem.
- `treeboot run --dry-run --skip-commands` plans the right file decisions and
  causes no file or configured-command effects.
- `file_planning.rs` produces concrete create, copy, symlink, metadata repair,
  delete, skip, and warning actions.
- `file_actions.rs` reduces those actions to per-operation counts.
- `file_execution.rs` either applies the actions or emits dry-run events.
- compact `FileOperationFinished` events already carry `changed`, `skipped`,
  `deleted`, `metadata_changed`, and `warnings` counts.
- `ActionPlan::warnings()` carries non-fatal validation warnings such as an
  include list that matched no source path. `check` and `config` report them;
  `run` text output intentionally does not.
- ownership-preservation warnings arise during mutation and currently exist only
  as `OwnershipWarning` reporter events.
- `run` currently supports text output only and returns exit `0` for both
  pending work and a no-op.
- configured commands use inherited stdio through `Command::status()`. A child
  can therefore write arbitrary bytes to stdout while it runs.

The implementation should expose a report from the planning boundary. It should
not make CLI consumers reconstruct state by collecting presentation events.

## CLI contract

### `treeboot plan`

Support these forms:

```sh
treeboot plan
treeboot plan --root /path/to/root-checkout
treeboot plan --config .treeboot.toml
treeboot plan --strict
treeboot plan --force
treeboot plan --skip-commands
treeboot plan --verbose
treeboot plan --format text
treeboot plan --format json
treeboot plan --format yaml
treeboot plan --json
treeboot plan --yaml
```

`plan` resolves the same worktree, config, environment, runtime policy, and
bootstrap `ActionPlan` as `run`. It plans the same concrete filesystem actions
that a run with the same `--strict` and `--force` values would plan.

It never writes files or spawns configured commands. It does not accept
`--dry-run` because planning is always side-effect-free.

By default, the report includes configured bootstrap commands as planned work.
`--skip-commands` omits them from both text and structured output. It does not
weaken config parsing or bootstrap semantic validation. This matches the current
`run --skip-commands` behavior, which builds the complete bootstrap plan before
skipping command execution.

Text output uses the same reporter events and message formatting as
`treeboot run --dry-run` for config discovery, file decisions, file-planning
warnings, and `treeboot: would run <label>` command lines. It additionally
prints the non-fatal validation warnings carried by the `ActionPlan`, because
`plan` is an inspection command. The existing `--verbose` behavior remains
available for text output.

Non-text formats reject `--verbose` as a usage error. The first structured
contract reports stable top-level operation summaries, not every recursively
expanded child action. This keeps output bounded for large directory copies
while preserving the information needed to decide whether Treeboot has work.

### Structured `treeboot run`

Add `--format text|json|yaml`, `--json`, and `--yaml` to explicit and implicit
`run` parsing.

Text remains the default and retains all current behavior.

JSON and YAML are accepted only when at least one of these is true:

- `--dry-run` prevents all file and configured-command effects;
- `--skip-commands` prevents configured commands from spawning.

This gives three supported structured modes:

| Invocation mode                 | Files                | Configured commands  | Report mode |
| ------------------------------- | -------------------- | -------------------- | ----------- |
| `run --dry-run`                 | planned, not applied | planned, not spawned | `dry_run`   |
| `run --skip-commands`           | applied              | omitted              | `execute`   |
| `run --dry-run --skip-commands` | planned, not applied | omitted              | `dry_run`   |

`treeboot run --json`, `treeboot run --yaml`, and equivalent non-text `--format`
uses without either safety option fail during argument validation, exit `2`,
write a diagnostic to stderr, and execute no Treeboot behavior.

Non-text run output also rejects `--verbose`. Structured summaries already carry
the material counts, and accepting a flag that does not change the document
would be misleading.

Structured run mode replaces the stdout reporter with a report collector and
disables interactive progress. The collector retains execution-time ownership
warnings in the final document instead of printing them as text. Fatal errors
use the existing stderr and non-zero exit behavior.

Treeboot must buffer the complete final document before writing any stdout. For
an executing file-only run, it must preflight every path reachable from the
prepared concrete actions before mutation, including expanded child targets that
can later appear in an ownership warning. It then applies the prepared groups,
records execution warnings, serializes the final document to a buffer, and
writes that buffer only after successful execution. A later execution error can
still leave the same partial filesystem effects as text mode, but stdout remains
empty rather than containing a partial document.

### Exit status

Successful plan and run reports exit `0` whether or not file changes exist.
Consumers inspect `has_file_changes`.

Do not add a changes-as-exit-status mode in this change. Treeboot already uses
exit `1` for runtime failures and exit `2` for CLI usage failures. Assigning a
third code for pending work would be unconventional, while reusing either
existing code would be ambiguous.

## Structured report

`treeboot plan`, structured `treeboot run --dry-run`, and structured file-only
execution share one report shape. JSON is shown below. YAML uses the same names
and values.

```json
{
  "mode": "plan",
  "context": {
    "root_path": "/repo",
    "worktree_path": "/repo-worktree",
    "default_branch": "main"
  },
  "action": {
    "kind": "config",
    "path": "/repo-worktree/.treeboot.toml"
  },
  "has_file_changes": true,
  "file_summary": {
    "changed": 4,
    "skipped": 2,
    "deleted": 1,
    "metadata_changed": 1,
    "file_warnings": 1
  },
  "files": [
    {
      "operation": "sync",
      "source": "shared",
      "target": "shared",
      "summary": {
        "changed": 4,
        "skipped": 2,
        "deleted": 1,
        "metadata_changed": 1,
        "file_warnings": 1,
        "expanded": true,
        "skip_reason": null
      }
    }
  ],
  "commands_skipped": false,
  "commands": [
    {
      "label": "Install packages: mise run setup",
      "allow_failure": false
    }
  ],
  "validation_warnings": [],
  "file_warnings": [
    {
      "path": "shared/current",
      "reason": "symlink target does not exist"
    }
  ],
  "execution_warnings": []
}
```

### Field semantics

`mode` is one of:

- `plan` for `treeboot plan`;
- `dry_run` for structured `treeboot run --dry-run`;
- `execute` for structured `treeboot run --skip-commands` without dry-run.

`action` is one of:

```json
{ "kind": "missing_config" }
```

```json
{ "kind": "root_worktree_skipped" }
```

```json
{ "kind": "config", "path": "/repo-worktree/.treeboot.toml" }
```

Missing discovered config and a non-strict root-worktree invocation are
successful no-ops with empty `files`, empty `commands`, zero counts, and
`has_file_changes: false`. An explicitly requested missing config remains an
error. Strict-mode missing config and root-worktree behavior remains unchanged.

`file_summary` is the total of all top-level file-operation summaries. A
metadata repair increments both `changed` and `metadata_changed`, matching the
current `FileOperationSummary` behavior. `file_warnings` counts concrete
file-planning warnings but does not imply a file change.

`has_file_changes` is true exactly when `file_summary.changed > 0` or
`file_summary.deleted > 0`. Skips, optional missing sources, and warnings do not
make it true.

`files` preserves config declaration order. Each entry represents one top-level
normalized file operation. An operation with no visible decision, such as an
unchanged sync, remains present with zero counts. This makes the structured
report a complete inventory rather than an event transcript.

The structured report projects the existing `FileOperationSummary` into a
report-specific type instead of serializing the text-formatting type directly.
For an expanded operation, `skip_reason` is always null because the first of
many skip reasons is not a stable summary. An unexpanded operation that consists
of one skip carries that reason.

`commands` preserves config declaration order and contains the same labels used
by text dry-run output. `commands_skipped` is true when `--skip-commands` was
selected, and `commands` is then empty. Structured `execute` mode always has
`commands_skipped: true` because non-text execution cannot spawn configured
commands.

`validation_warnings` contains ordered human-readable `ActionPlan` warning
strings, matching the warning vocabulary used by `check`. These warnings are
reported by `plan` text output and all new structured reports. Existing text
`run` behavior remains unchanged.

`file_warnings` contains stable path and reason pairs produced by concrete file
planning. The list preserves planning order, and its length equals
`file_summary.file_warnings` and the sum of per-operation warning counts.

`execution_warnings` contains non-fatal path and reason pairs produced while
applying files, currently ownership-preservation warnings. It is empty in `plan`
and `dry_run` modes. Execution warnings do not alter the precomputed file
decision counts.

For the motivating editor check, `has_file_changes` is the complete decision. A
consumer interested in any planned bootstrap work checks
`has_file_changes || !commands.is_empty()`.

Path serialization follows the existing structured-output contract. Native
non-UTF-8 paths work in text mode; JSON/YAML fail before stdout is written.

## Core and module design

### Public core API

Add a command-shaped planning facade:

```rust
pub fn plan(
    options: PlanOptions,
    reporter: &mut dyn Reporter,
) -> Result<BootstrapReport>;
```

`PlanOptions` is `#[non_exhaustive]`, implements `Default`, and mirrors the
planning-affecting parts of `RunOptions`: `cwd`, `root`, `environment`,
`config`, `strict`, `force`, `verbose`, and `skip_commands`. Output-format
selection remains a CLI concern. `verbose` stays in the core options because it
selects compact versus detailed reporter events, matching `RunOptions`.

`BootstrapReport` and its nested report structs are serializable,
`#[non_exhaustive]`, and constructible through the planning facade rather than
public struct literals. The report uses `WorktreeSnapshot` rather than the full
`Worktree`, so it does not expose or serialize the child environment.

The reporter-aware signature keeps text `plan` on the existing dry-run event and
message path, including verbose child actions and planning progress. The CLI
passes `StdoutReporter` for text and a non-printing collector for JSON/YAML.
`plan` emits a new additive `OutputEvent::ValidationWarning { message }` after
`ConfigDetected` and before file decisions. Its text form is
`treeboot: warning: <message>`, matching `check`.

Do not add fields to the existing constructible `RunReport`. Add an additive
`run_detailed` facade that returns the shared structured report while preserving
the existing `run` signature and return type:

```rust
pub fn run_detailed(
    options: RunOptions,
    reporter: &mut dyn Reporter,
) -> Result<BootstrapReport>;
```

The current `run` function delegates to the same internal run flow and projects
the existing `RunReport`. Text CLI callers retain the current public behavior.
The CLI uses `run_detailed` when it needs a structured report.

`run_detailed` always preflights every prepared action path for structured
serialization. It is the structured-consumer facade, so this needs no new field
on the existing exhaustive `RunOptions`. The text CLI continues to call `run`
and retains native non-UTF-8 path support. `plan` does not preflight paths in
core because it never mutates; its CLI serializer can fail before stdout in the
usual way.

Use one core `BootstrapReport`; do not add a `PlanReport` alias or wrapper.
`mode` identifies the CLI invocation rather than domain state, so a thin
CLI-owned serialization wrapper adds it to the top-level JSON/YAML document. The
core report owns everything else in the shared shape.

Project the current `FileOperationSummary` into dedicated report structs rather
than deriving its JSON contract directly from a type that also owns text
formatting. The public API must not expose private concrete `FileAction`
variants or allow a caller to execute a saved filesystem snapshot later.

### Shared preparation

Extract one internal bootstrap preparation path that owns:

1. runtime-policy resolution;
2. worktree/root/config discovery;
3. config loading and normalization;
4. validated `ActionPlan` construction;
5. concrete file-operation group planning;
6. cross-operation warning calculation;
7. report-summary and validation-warning construction;
8. structured action-path preflight for the detailed run path.

`plan`, dry-run, and execution all call this path. Execution applies the
prepared groups immediately within the same invocation. The public report is a
snapshot for inspection, not an executable plan. A later confirmed run plans
again from current filesystem state.

Refactor `file_operations.rs` so group preparation and group execution are
separate internal operations. Keep `PlannedFileOperationActions` and
`FileAction` private. Expose only report types that contain display paths,
operation kinds, summaries, command labels, and warnings. Immediate execution
returns execution warnings as data in addition to reporting them, so
`run_detailed` can include ownership warnings without scraping presentation
events.

Preserve the existing safety rule that every file-operation group is planned
before the first mutation. Structured serialization preflight runs after all
groups are prepared and before execution.

### Ownership map

| Module               | New responsibility                                                                               |
| -------------------- | ------------------------------------------------------------------------------------------------ |
| `plan.rs`            | Public `PlanOptions`, shared bootstrap report types, and reporter-aware side-effect-free facade. |
| `run.rs`             | Existing run compatibility facade, detailed run facade, and shared bootstrap dispatch.           |
| `file_operations.rs` | Prepare all concrete groups, preflight report data, then preview or apply those exact groups.    |
| `file_actions.rs`    | Continue to own private concrete actions and summary/warning reduction.                          |
| `file_execution.rs`  | Apply prepared groups, return execution warnings as data, or emit text dry-run events.           |
| `output.rs`          | Existing text events plus additive validation-warning event and message formatting.              |
| `commands/run.rs`    | Run option validation and text/structured rendering selection.                                   |
| `commands/plan.rs`   | Plan arguments and text/structured rendering selection.                                          |
| `commands/output.rs` | Reuse complete-document JSON/YAML serialization.                                                 |

Update `docs/ARCHITECTURE.md` to show `plan` as view-only and to describe the
shared preparation boundary before preview or immediate execution.

## Spec and registry updates

The spec change crosses several exact-reference checks. Treat these as one
atomic documentation/test update:

- Rename `CLI surface: Fifteen subcommands, one default path` to the stable,
  count-free `CLI surface: Subcommands and one default path` heading.
- Replace the resulting old anchor in portable case metadata,
  `crates/treeboot-spec/src/cases/closure.rs`, and the human-report assertion in
  `crates/treeboot-spec/tests/cli.rs`.
- Add `plan` to the command synopsis, command descriptions, option-scope table,
  root-checkout behavior, missing-config behavior, structured-output section,
  and operator output examples.
- Move `run` out of the spec's unconditional text-only command list and define
  its value-dependent structured-output restriction.
- Add `plan` to both portable version-flag command inventories and to completion
  expectations.
- Add portable plan/run structured cases to the hand-maintained
  `crates/treeboot-spec/src/cases/generated.rs` registry, update the audited
  unique source-key count in `scripts/check-spec-cases.sh`, and keep closure
  counts exact.
- Update the reference-only allowlist only if implementation-specific coverage
  cannot live in portable conformance. `scripts/check-harness.sh` runs the
  registry audit through `scripts/check-spec-cases.sh`.

This work does not rename anchors merely to update a count. The count-free
heading avoids repeating the same cascade when Treeboot gains another command.

## Implementation sequence

1. **Settle the portable contract.** Update the spec with the sixteenth
   subcommand, CLI option matrix, structured report schema, command-stdio
   restriction, no-op/error behavior, and exact `has_file_changes` rule. Bump
   the spec and README references to `2.6.0`. Rename the count-bearing CLI
   heading to `CLI surface: Subcommands and one default path`, then update its
   anchor in the generated case registry, closure metadata, and `treeboot-spec`
   report assertions.

2. **Separate preparation from effects.** Extract all-group file preparation
   from `apply_file_operations`, retain private concrete actions, and prove text
   run and manual copy/symlink/sync behavior is unchanged before adding new
   command paths. Preserve the public `Executor` flow and compact/verbose event
   selection.

3. **Add report types and planning facade.** Build total and per-operation
   reports from the prepared groups, snapshot commands and warnings, and expose
   the reporter-aware `plan` facade through `treeboot-core`.

4. **Add detailed run reporting.** Preserve `run` and `RunReport`, add the
   detailed facade, and make preview/application consume the same prepared
   groups that produced the report. Return ownership warnings as execution data
   while retaining their existing text reporter events.

5. **Add CLI parsing and rendering.** Register `plan`, add structured options to
   explicit and implicit run forms, and route text output through the existing
   reporter while buffering JSON/YAML documents. Use ordinary Clap conflicts
   where flags are sufficient. Use a post-parse validation step that constructs
   a `clap::Error` with usage exit `2` for value-dependent rules such as
   `--format json` requiring dry-run or command skipping and non-text formats
   rejecting `--verbose`. Run this validation before Git or config discovery.
   Reject implicit-run output flags placed before an explicit subcommand rather
   than silently ignoring them.

6. **Update user and API documentation.** Document human preview, editor
   integration, file-only confirmation, structured-output restrictions, public
   core usage, completions, and the current architecture.

7. **Refresh generated and audited registries.** Regenerate the spec-version
   asset and completion artifacts. Manually add portable cases to
   `crates/treeboot-spec/src/cases/generated.rs`, adjust its audited unique-key
   count, add `plan` to both version-subcommand inventories, and update the
   reference-only allowlist/count in `scripts/check-spec-cases.sh` only if a new
   reference implementation test is justified. `mise run generate` does not own
   the conformance case registry.

## Test strategy

### Core behavior

Add focused tests for:

- missing config, explicit missing config, root worktree, and strict variants;
- copy and symlink missing-target changes versus existing-target skips;
- unchanged and changed sync files and directories;
- sync deletes and metadata-only repairs;
- optional missing sources and planning warnings;
- force and strict parity between plan, dry-run, and execution;
- command ordering, labels, `allow_failure`, and `--skip-commands` omission;
- validation, file-planning, and execution warning categories staying distinct;
- ownership warnings appearing in successful detailed execute reports through an
  injected internal execution-warning seam;
- `execution_warnings` remaining empty by construction on non-Unix hosts;
- total counts matching the sum of ordered operation summaries;
- `has_file_changes` excluding skips and warnings;
- all groups planning before any execution begins;
- no public path for executing a stale `BootstrapReport`.

Use one representative perturbation to prove that plan and execution share the
same group-preparation path. The test should fail at a decision or summary
assertion rather than at setup or compilation.

Use one mixed fixture to compare the three views of the same decisions:
`plan --json`, `run --dry-run --json`, and a subsequent
`run --skip-commands --json`. Include an existing copy target, a sync update and
delete, a metadata repair, and a dangling-target symlink warning. The first two
reports must agree on file decisions; the third must report those decisions as
applied and leave a follow-up plan with `has_file_changes: false`.

### Portable CLI conformance

Add `treeboot-spec` cases covering:

- `plan` help, version propagation, completion exposure, and option conflicts;
- exact JSON and YAML shapes and text parity;
- `plan --skip-commands --json` with changes, skips, warnings, and no work;
- explicit and implicit `run --dry-run --json`;
- `run --skip-commands --json` applying files while never spawning a failing
  configured command;
- combined dry-run and skip-commands behavior;
- rejection of structured run without a command-suppression mode;
- rejection of `--verbose` with non-text output;
- acceptance of `plan --format text --verbose` with detailed child actions;
- rejection of implicit-run output flags placed before an explicit subcommand;
- invalid config and filesystem-planning errors leaving stdout empty;
- non-UTF-8 paths retaining text support and failing structured output before
  stdout or mutation;
- a configured command that would print JSON-like stdout never spawning during
  structured dry-run;
- successful execute reports exposing the `execution_warnings` array without
  contaminating stderr or invalidating JSON/YAML;
- text `run`, text dry-run, and existing manual commands retaining their exact
  durable behavior.

Do not require portable conformance to trigger a real ownership warning. That
requires arranging an ownership-changing operation whose `chown` fails with
`PermissionDenied`, which an unprivileged cross-platform fixture cannot do
reliably. Keep the warning-production test in core behind an injected seam. A
host-capability-gated conformance case may supplement it when the precondition
can be arranged and must report an explicit runtime skip otherwise.

For an executing structured file-only run, cover both full success and a later
execution error. The error case must assert empty stdout and the same permitted
partial-file-effect boundary as text execution.

### Reference implementation and public API

Add focused tests for:

- `PlanOptions`, non-exhaustive report types, and `run_detailed` exports;
- text reporter progress remaining text-only;
- JSON/YAML serialization occurring as one buffered write;
- structured path preflight covering expanded child action and warning paths;
- argument conflicts failing before Git/config discovery;
- CLI-spec anchor references and both version-subcommand inventories remaining
  complete;
- manual `copy`, `symlink`, and `sync` plus public `Executor` behavior remaining
  unchanged across the preparation refactor;
- the old `run` signature and `RunReport` behavior remaining source-compatible.

### Validation commands

During implementation, use affected tasks first:

```sh
mise run format:rust:check
mise run test:core
mise run test:cli
mise run test:spec
mise run test:conformance
mise run generate:check
```

Inspect touched-module gaps with:

```sh
mise run coverage:missing
```

Run the ordinary handoff gate on the intended final head:

```sh
mise run check
```

Because this changes the portable CLI contract, public core API, generated
metadata, and architecture documentation, run the broader final verification:

```sh
mise run verify
```

## Risks and controls

- **Plan and run drift.** One internal group-preparation path produces both
  reports and execution input. Conformance cases compare plan, dry-run, and
  applied decisions.
- **Structured stdout contamination.** Non-text execution requires
  `--skip-commands`; dry-run never spawns commands. The implementation does not
  redirect or capture child stdio.
- **Execution-time warning loss.** The detailed execution path returns ownership
  warnings as report data while the existing text path continues to emit
  `OwnershipWarning` events.
- **Filesystem races.** Reports are snapshots. A later run always re-plans.
  Execution applies groups immediately and retains existing live boundary and
  source-symlink checks.
- **Large trees.** Structured output reports one entry per declared operation,
  not every expanded child action. Planning cost remains proportional to the
  same traversal needed by dry-run.
- **Partial mutation on execution failure.** Structured file-only execution
  retains normal run semantics but never writes a partial report to stdout.
- **Public API breakage.** Keep `RunReport` unchanged and add new non-exhaustive
  detailed report types and facades.
- **Path encoding.** `run_detailed` preflights every prepared action path before
  mutation. `plan` relies on complete-document serialization before stdout. Text
  `run` and text `plan` retain native path support.
- **Spec metadata drift.** Replace the count-bearing CLI anchor everywhere in
  one change, update the hand-maintained portable registry and audited counts,
  and run `harness:check` through the normal gates.

## Rejected alternatives

### Teach `check` to report pending work

Rejected because `check` validates both bootstrap and teardown semantics while
pending work is a current-filesystem bootstrap question. Combining them would
make `check ok` ambiguous and force existing structured consumers to handle a
new operational meaning.

### Make `plan` a text-only alias for `run --dry-run`

Rejected because it leaves integrations parsing lifecycle prose and preserves
the original gap. The alias is useful only if it exposes a stable report.

### Ship structured output only on `plan`

Rejected because the settled scope also calls for structured `run` output. A
structured dry-run lets existing `run --dry-run` integrations migrate without
changing commands, and structured file-only execution gives the editor a clean
post-confirmation result. The command-suppression rule and explicit execution
warning reporting contain the added risk.

### Allow unrestricted `treeboot run --json`

Rejected because configured commands inherit stdout. Capturing child output
introduces buffering, byte encoding, size limits, interactive-process, and
failure-reporting policy. Redirecting stdout changes existing command behavior.
Neither belongs in this feature.

### Return pending work through exit status

Rejected for the first version because Treeboot already assigns `1` to runtime
errors and `2` to usage errors. The structured boolean is unambiguous for the
editor integration that motivated the command.

## Non-goals

- Automatically running Treeboot from an editor extension.
- Designing VS Code prompts, workspace-trust handling, multi-root deduplication,
  or extension packaging.
- Structured output for teardown or manual copy/symlink/sync commands.
- Capturing, redirecting, truncating, or encoding configured command output.
- Persisting a plan or executing it later without re-planning.
- Watching the filesystem or config for continuous plan refresh.
- Adding a quiet or changes-as-exit-status mode.
- Exposing every expanded child file action in structured output.

## Unresolved questions

None. The structured-run safety restriction, report granularity, exit behavior,
and public API compatibility approach are settled by this plan.

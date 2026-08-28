# Plan command and structured run output

Status: approved and implemented.

This document records the implementation choices and test closure for the
Treeboot planning and structured-run feature. The portable behavior belongs in
the
[`treeboot plan` and structured `treeboot run` specification](../../crates/treeboot-spec/SPEC.md#treeboot-plan-and-structured-treeboot-run).
The current code paths and module responsibilities belong in
[`docs/ARCHITECTURE.md`](../ARCHITECTURE.md#entry-points-command-surface).

## Outcome

Treeboot now has a side-effect-free planning command for people and tooling that
need to know whether bootstrap work is pending. Safe run modes can return the
same machine-readable report before or after file application. Existing text run
behavior and the public `RunReport` API remain compatible.

The editor integration that motivated the work can inspect the report's file
change signal and decide whether to prompt. Treeboot does not own the editor
prompt, workspace trust, or scheduling policy.

## Decisions retained

- Keep `plan` distinct from `check`. Planning compares the current filesystem;
  checking validates bootstrap and teardown semantics.
- Build plan, dry-run, and execution reports from one prepared set of concrete
  file-operation groups. A confirmed run plans again rather than executing a
  saved report.
- Permit structured run output only when configured commands cannot write to
  inherited stdout. Capturing or redirecting child output remains out of scope.
- Report one summary per declared operation instead of exposing expanded child
  actions. This keeps reports bounded without hiding whether file work exists.
- Preserve the existing `run` and `RunReport` API. Add planning and detailed-run
  APIs for consumers that need the full report.
- Buffer structured output before writing stdout. Detailed execution preflights
  reportable paths before mutation, while later execution errors retain normal
  partial-file-effect semantics.
- Keep execution warnings as report data on the detailed path and as reporter
  events on the compatible text path.

## Implementation record

1. The portable specification moved to version `2.6.0`, added the planning and
   structured-run contract, and adopted a count-free CLI heading.
2. Core file handling separated preparation from preview and execution. Private
   concrete actions remain internal and feed public report summaries.
3. Core added the planning facade, detailed run facade, report types, validation
   warnings, execution-warning collection, and structured path preflight.
4. The CLI added `plan`, shared format handling, pre-discovery validation, and
   buffered JSON/YAML rendering. Flattened implicit-run options are rejected
   when an explicit subcommand follows.
5. User docs, completions, architecture notes, generated metadata, and the
   portable conformance registry were updated with the feature.

For current execution ownership and data flow, see
[`Applying actions`](../ARCHITECTURE.md#applying-actions).

## Test closure

Core and public API coverage verifies:

- shared preparation across planning, dry-run, and execution;
- report aggregation, ordering, warnings, skips, metadata repairs, and deletes;
- missing-config, root-worktree, strict, force, and command-skipping behavior;
- compatibility of the existing run return type and public execution APIs;
- structured path preflight, buffered writes, and injected execution warnings.

Portable conformance coverage verifies:

- text, JSON, and YAML planning and run modes;
- exact option conflicts and validation before discovery;
- rejection of implicit-run options before explicit subcommands;
- success, no-op, validation failure, planning failure, and execution failure;
- file-only execution without command spawning;
- report parity across planning, dry-run, and execution;
- native path formatting and atomic failure for unrepresentable structured
  paths;
- the permitted partial-effect boundary when execution fails after an earlier
  file action.

The shared-preparation behavioral cluster received one targeted perturbation. It
failed at the intended report-decision assertion, was restored exactly, and then
passed. Platform-dependent ownership-warning production stays behind an injected
core test because no reliable portable fixture can force it.

Repository checks cover formatting, linting, generated assets, registry
inventories, the standalone spec package, cross-platform builds, and release
metadata. The normal final gates remain `mise run check` and `mise run verify`
as described in the repository validation guide.

## Non-goals

- Editor extension behavior or packaging.
- Persisted plans or later execution without re-planning.
- Structured output for teardown or manual file commands.
- Capturing, truncating, or encoding configured command output.
- Filesystem watching or a changes-as-exit-status mode.

## Unresolved questions

None.

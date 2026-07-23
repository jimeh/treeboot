# Teardown Commands And Public Config Evolution Plan

Status: implemented.

This plan adds an explicit worktree teardown phase while using the same
compatibility boundary to make Treeboot's public normalized config structs
forward-compatible with additive fields.

The observable behavior in this plan must be incorporated into
[`docs/SPEC.md`](../SPEC.md) before implementation is considered complete.
Implementation placement and public API structure must remain consistent with
[`docs/ARCHITECTURE.md`](../ARCHITECTURE.md).

## Goals

- Let projects declare commands that run before a linked Git worktree is
  removed.
- Add a `treeboot teardown` command that is safe for both interactive use and
  removal automation.
- Keep Git worktree removal outside Treeboot.
- Reuse existing command declaration, validation, environment, execution, and
  failure semantics.
- Keep bootstrap and teardown semantic planning independent after whole-config
  parsing and normalization succeed, so a bootstrap-only planning failure does
  not prevent cleanup.
- Make additive fields on Treeboot's public normalized config structs
  non-breaking after this release.
- Preserve useful programmatic construction paths after applying
  `#[non_exhaustive]`.

## Non-Goals

- Removing a Git worktree, deleting its branch, or wrapping
  `git worktree remove`.
- Installing or enforcing a Git pre-removal hook. Git worktree removal remains
  able to bypass Treeboot.
- Declarative teardown file operations. Teardown is command-only in this
  version.
- Automatically reversing bootstrap command order or inferring inverse
  operations.
- Running independent teardown commands after a fatal command failure. Commands
  retain the existing sequential stop-on-failure behavior; projects can use
  `allow_failure` or a task-runner command when they need aggregation.
- Applying `#[non_exhaustive]` to every public options and report struct in
  `treeboot-core`. This change is limited to the normalized config graph and the
  resolved `Worktree` context carried by `LoadedConfig`.

## Settled Terminology

Use **teardown** throughout the CLI, config, core API, documentation, and
output.

- `teardown` pairs naturally with bootstrap.
- `cleanup` is too broad and could imply routine cache or file cleanup.
- `pre_remove` implies an automatically installed removal hook.
- `remove` and `destroy` imply that Treeboot deletes the worktree.
- `deinit` is less familiar and does not improve precision.

## User-Facing Config Contract

Add compact `teardown_commands` entries and verbose `[[teardown_command]]`
entries:

```toml
commands = [
  "mise install",
  { name = "Create database", run = "mise run db:create" },
]

teardown_commands = [
  { name = "Stop services", run = "docker compose down" },
  { name = "Drop database", run = "mise run db:drop" },
]

[[teardown_command]]
name = "Remove preview environment"
program = "mise"
args = ["run", "preview:destroy"]
allow_failure = false
```

Both forms accept the existing command fields:

- `name`
- exactly one of `run` or `program`
- `args` when `program` is used
- `cwd`
- `env`
- `allow_failure`

String entries remain shorthand for shell commands. Compact entries run before
verbose entries, matching `commands` followed by `[[command]]`.

The normalized `Config` representation and structured `treeboot config` output
gain:

```json
{
  "commands": [],
  "teardown_commands": []
}
```

An omitted teardown declaration normalizes to an empty list. New Treeboot
versions therefore continue to read all existing config files unchanged. Old
Treeboot versions continue to reject configs that use the new keys because the
config parser deliberately denies unknown fields.

Add an empty teardown section to newly generated starter configs:

```toml
teardown_commands = [
]
```

## CLI Contract

Add these supported forms:

```sh
treeboot teardown
treeboot teardown --dry-run
treeboot teardown --yes
treeboot teardown --worktree ../feature-branch
treeboot teardown --worktree ../feature-branch --yes
treeboot teardown --root /path/to/root-checkout
treeboot teardown --config .treeboot.toml
```

### Options

| Option                  | Behavior                                                                                                                                                                |
| ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--worktree <path>`     | Selects the linked worktree to tear down. Defaults to the process working directory. The path may point anywhere inside the worktree and resolves to its Git top level. |
| `-r`, `--root <path>`   | Overrides root-checkout discovery, matching other context-aware commands.                                                                                               |
| `-c`, `--config <path>` | Selects one config relative to the target worktree and skips normal config discovery.                                                                                   |
| `-n`, `--dry-run`       | Validates and reports teardown commands without prompting or spawning them.                                                                                             |
| `--yes`                 | Long-only explicit approval that suppresses the interactive prompt.                                                                                                     |

Do not add `--force`. That name already controls file-operation replacement and
must not acquire an unrelated confirmation meaning.

Do not add `--strict` in the first implementation. Teardown has no file
operations to which strict conflict semantics apply. Callers that require a
specific config can pass `--config`, which already fails when the requested file
is absent.

Teardown does not resolve the bootstrap `RuntimePolicy`. `ConfigRuntimeOptions`,
`TREEBOOT_STRICT`, and the dangerous file-boundary environment settings affect
bootstrap file planning, not command-only teardown. They remain part of the
parsed config and explicit `EnvironmentInput`, but they do not make a missing
discovered teardown config fatal, permit a root-checkout target, or otherwise
change teardown behavior.

### Target And Discovery Behavior

1. Resolve the selected path through normal Git worktree discovery.
2. Resolve the root checkout and standard Treeboot environment for the target
   worktree.
3. Reject the main/root checkout before reading confirmation input. Teardown is
   only valid for a linked worktree.
4. Resolve explicit or discovered config from the selected worktree.
5. Parse and normalize the whole TOML document.
6. Plan and validate only teardown commands for teardown execution.

Whole-document parsing and construction of the normalized `Config` are a shared
prerequisite, not a phase-specific operation. Any syntax, type,
declaration-shape, unknown-field, or declaration-normalization error returned by
`Config::load` anywhere in the document is fatal to both phases. Bootstrap and
teardown independence begins only after `Config::load` succeeds; semantic
planning-time validation and path-containment failures remain phase-specific.

Missing discovered config is a successful no-op with the existing
`treeboot: no config detected` output. An explicitly requested missing config
remains an error.

A valid config with no teardown commands is a successful no-op, prints
`treeboot: no teardown commands configured`, and never prompts.

### Confirmation Behavior

Confirmation occurs only after context discovery, config parsing, and teardown
planning have succeeded. The prompt must identify the canonical worktree path
and number of commands:

```text
Run 2 teardown commands for /repo/worktrees/feature?
These commands may delete resources outside the worktree. [y/N]
```

- Write the prompt to stderr and flush it before reading input.
- Treat stdin as interactive only when `std::io::IsTerminal` reports a terminal.
- Accept `y` and `yes`, case-insensitively, after trimming whitespace.
- Treat every other response, an empty line, and EOF as refusal.
- Refusal exits non-zero and runs nothing.
- Non-interactive execution without `--yes` exits non-zero, runs nothing, and
  tells the caller to rerun with `--yes`.
- `--yes` bypasses the prompt but does not bypass discovery, parsing, or
  validation.
- `--dry-run` never prompts and never requires `--yes`.
- Missing config and empty teardown plans never prompt.

Refusal must be unsuccessful so this safe composition cannot proceed to removal
after a declined teardown:

```sh
treeboot teardown --worktree "$path" &&
  git worktree remove "$path"
```

### Execution Behavior

- Execute teardown commands sequentially in declaration order.
- Default `cwd` is the selected worktree root.
- Relative `cwd` values resolve from the selected worktree.
- Commands inherit the existing `TREEBOOT_*` environment and compatibility
  aliases for the selected worktree.
- Per-command `env` merging and Treeboot-owned-variable protection remain
  unchanged.
- `allow_failure = false` stops at the first cwd, spawn, or exit failure and
  returns non-zero.
- `allow_failure = true` reports the failure and continues, matching bootstrap
  command semantics.
- Re-resolve and revalidate the live command cwd immediately before every spawn.
  Approval must not weaken the existing protection against cwd symlink
  retargeting.
- A live cwd revalidation failure is an ordinary command-start failure. With
  `allow_failure = false` it is fatal; with `allow_failure = true` it emits the
  teardown allowed-failure warning and continues. In both cases the rejected
  command is never spawned.
- Never apply configured file operations or run bootstrap `commands`.
- Never remove the worktree.

Suggested durable output:

```text
treeboot: config detected /repo/worktrees/feature/.treeboot.toml
treeboot: teardown run Stop services: docker compose down
treeboot: teardown run Drop database: mise run db:drop
```

Dry-run output:

```text
treeboot: config detected /repo/worktrees/feature/.treeboot.toml
treeboot: teardown would run Stop services: docker compose down
treeboot: teardown would run Drop database: mise run db:drop
```

Allowed failure output should identify the teardown phase:

```text
treeboot: warning: teardown command Stop services: docker compose down \
failed with exit status 1
```

## Planning And Execution Architecture

Keep the existing `ActionPlan` as the bootstrap plan. Do not place teardown
commands in `ActionPlan::commands()`, add a phase field to existing commands, or
make bootstrap execution filter a mixed list.

Add a command-only `TeardownPlan` with private fields and public accessors for:

- resolved `Worktree` context
- manifest path
- shared validated command representation

The plan must be constructible only through validation. Mark it
`#[non_exhaustive]` if any fields are public; preferably keep all fields private
as `ActionPlan` does.

Keep one trait-free internal command planning component. Generalize the existing
`plan_commands` free function to return a small `PlannedCommands` value, or an
equivalent private representation, from a supplied command slice:

```text
plan_commands(path, operations, context) -> PlannedCommands
```

Both `ActionPlan` and `TeardownPlan` embed that representation. Do not give
either phase a separately implemented planner. The shared planner applies:

- normalize default or declared cwd
- keep cwd inside the worktree
- reject overrides of Treeboot-owned environment variables
- retain source-span attribution

Keep the phases independent:

- `treeboot run` builds and executes only the bootstrap `ActionPlan`.
- `treeboot teardown` builds and executes only `TeardownPlan`.
- A missing required bootstrap file, strict sync conflict, bootstrap command cwd
  that normalizes and then fails worktree containment, or bootstrap
  Treeboot-owned environment override must not prevent a valid teardown plan
  from cleaning up.
- Invalid TOML, malformed declarations, unknown fields, or declared paths that
  cannot be normalized while constructing `Config` remain fatal before either
  phase is planned.
- Invalid teardown command planning remains fatal to teardown execution.
- `treeboot check` and `treeboot doctor` validate both plans because those
  commands inspect the complete config contract.
- `treeboot config` prints both collections and reports phase-specific
  validation warnings without changing its successful parse-only exit status.

### Complete Config Phase Validation

Add one internal `validate_manifest_phases` helper, or equivalently named
component, that independently evaluates both semantic plans after config parsing
and normalization:

```text
validate_manifest_phases(path, config, context, bootstrap_options)
  -> ConfigPhaseValidation {
       bootstrap: plan-or-error,
       teardown: plan-or-error,
     }
```

The result preserves both outcomes even when both phases fail and orders
bootstrap before teardown deterministically. It is the only component that
coordinates complete-config validation.

- `treeboot check` maps any failed phase to one typed aggregate validation error
  whose display includes every failed phase in deterministic order.
- `treeboot doctor` maps the same result to separate bootstrap and teardown
  diagnostics.
- `treeboot config` maps it to phase-labelled warnings while retaining its
  successful parse-only exit status.

These commands must not each call both plan constructors and independently
invent aggregation, labels, or error precedence.

### Prepare, Confirm, Execute

The core layer must support a two-stage teardown flow:

1. Prepare once: discover, load, parse, normalize, plan, and produce a validated
   immutable teardown result.
2. Execute that same prepared result after CLI approval.

Do not reload or reparse the config after confirmation. This avoids approving
one command set and executing a newly read command set.

The prepared result needs to distinguish:

- missing discovered config
- valid config with no teardown commands
- ready `TeardownPlan`

The exact public type names may follow existing `Action` and `Report`
conventions, but the invariants must be explicit and fields that should not be
externally fabricated must remain private.

Preparation accepts a `Reporter` and owns discovery and no-op output:

- `ConfigDetected`
- `NoConfigDetected`
- `NoTeardownCommandsConfigured`

Execution owns only command lifecycle output. Confirmation and refusal remain
binary concerns and emit no core execution events. Specify this division in the
public API documentation so dry-run, empty-plan, and declined transcripts cannot
vary according to where an implementer happens to report discovery.

Keep terminal detection and prompt I/O in the binary crate. Core preparation and
execution must not read stdin, inspect terminal state, or depend on the
`console` crate.

### Shared Command Runtime

Generalize the internal command executor to the matching trait-free shape:

```text
execute_commands(context, planned_commands, phase, options, reporter)
```

It accepts:

- a `Worktree` context
- the shared `PlannedCommands` representation
- dry-run state
- an internal command phase used only to choose output events

`Executor` remains the bootstrap file-plus-command orchestrator. It calls the
shared runtime after file execution. `teardown.rs` owns teardown preparation,
no-op actions, and execution of a prepared teardown plan, and calls the same
runtime directly. Do not add a second public executor or teach `Executor` to
become a mixed phase dispatcher.

Centralize `CommandPhase -> OutputEvent` selection beside the shared runtime so
the process-spawn, environment, cwd, failure, and reporting control flow remains
single-source.

Do not add fields to existing `OutputEvent::CommandStarted`, `CommandWouldRun`,
or `CommandAllowedFailure` variants. Although `OutputEvent` is non-exhaustive,
changing the fields of an existing variant can still break downstream variant
matches.

Add new output variants instead:

- `TeardownCommandStarted`
- `TeardownCommandWouldRun`
- `TeardownCommandAllowedFailure`
- `NoTeardownCommandsConfigured`

Existing bootstrap events and durable messages remain byte-for-byte unchanged.

### CLI Error Boundary

Confirmation refusal and terminal/prompt errors are CLI concerns rather than
config, planning, or execution errors. Introduce a small binary-local error type
that wraps `treeboot_core::Error` and represents:

- confirmation required for non-interactive execution
- teardown declined
- prompt input/output failure

Update `run_cli` and `main` to print and map this type while preserving current
core error text and exit code behavior. Confirmation-required and declined
outcomes use exit code `1`; Clap argument errors remain exit code `2`.

Add `Worktree::is_root()` and use it in the existing run, manual, check, and
doctor paths as well as teardown. Core should add a typed root-worktree teardown
error because rejecting the root checkout is part of reusable teardown
preparation, not presentation.

Do not place root rejection in `plan_commands`, `validate_manifest_phases`, or
`TeardownPlan::from_manifest`. Inspection commands retain their existing
strict/non-strict root behavior and may validate declarations without making
root-checkout inspection unconditionally fatal.

## Public Config Struct Compatibility

Apply `#[non_exhaustive]` to the normalized public config graph in the same
breaking release:

| Type                   | Treatment                                                                                  | Construction after the change                                                 |
| ---------------------- | ------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------- |
| `LoadedConfig`         | Mark non-exhaustive; retain readable public fields.                                        | Output-only; constructed by Treeboot.                                         |
| `Worktree`             | Mark non-exhaustive; retain readable public fields; add `is_root`.                         | Discover through Treeboot, or use a documented `from_parts` constructor.      |
| `Config`               | Mark non-exhaustive, add `teardown_commands`, and implement `Default` for an empty config. | Parse/load, or create with `Config::default()` and mutate public collections. |
| `ConfigRuntimeOptions` | Mark non-exhaustive; retain `Default`.                                                     | Create with `Default` and set public options.                                 |
| `FileOperation`        | Mark non-exhaustive; retain readable public fields.                                        | Use existing manual normalization or new stable constructors/builders.        |
| `CommandOperation`     | Mark non-exhaustive; retain readable public fields.                                        | Use new shell/direct constructors and modifier methods.                       |
| `SourceSpan`           | Mark non-exhaustive; retain readable public fields.                                        | Use a new documented constructor rather than a literal.                       |

Do not mark the closed config vocabulary enums non-exhaustive:

- `FileOperationKind`
- `SyncCompare`
- `SymlinkMode`
- `MetadataField`
- `CommandKind`

They intentionally model closed domains whose exhaustive matching is useful.
Changing one of those domains remains a deliberate compatibility decision.

Do not include `ConfigOptions` in this scope. It is command input rather than
part of the normalized config graph. A future project-wide policy for options
and report structs should be handled consistently rather than changing only one
options type here.

### Stable Construction Replacements

`#[non_exhaustive]` prevents downstream struct literals. Preserve the current
programmatic use cases through constructors rather than forcing callers to parse
TOML.

Add `SourceSpan::new(start, end, line, column)`.

Add
`Worktree::from_parts(root_path, worktree_path, default_branch, environment)`
for callers that currently construct synthetic resolved contexts. The
constructor preserves the existing ability to supply all public data without
promising discovery or filesystem validation. Add `Worktree::is_root()` as the
single root-checkout predicate.

Add stable `CommandOperation` constructors:

```rust
CommandOperation::shell(run, declaration)
CommandOperation::direct(program, args, declaration)
```

Provide chainable modifiers or setters for:

- name
- declared and resolved cwd as one coherent update
- environment entries
- allow-failure policy

Add stable `FileOperation` construction through operation-specific constructors
that take a `Worktree`, declared paths, and source attribution, then derive
normalized paths through shared internal helpers:

```rust
FileOperation::copy(...)
FileOperation::symlink(...)
FileOperation::sync(...)
```

Do not make callers supply both declared and independently computed normalized
paths. Extract one internal normalized-operation builder used by config parsing,
manual operations, and these public constructors. The frontends remain
responsible for their context-specific inputs and error attribution:

- config parsing supplies config-level `default_ignore` before construction
- manual operations supply manual CLI defaults and required-path behavior
- standalone public constructors have no config-level `default_ignore` and use
  operation-local defaults

After those inputs are explicit, all three paths share path resolution,
per-operation setting validation, default installation, and final
`FileOperation` assembly. Provide modifiers only for fields valid for the
selected operation, or return typed validation errors when an incompatible
setting is requested.

Continue to recommend `FileOperation::from_manual_options` for callers that want
Treeboot to derive paths and operation settings from a `Worktree`.

Do not rely on `..Default::default()` as the compatibility mechanism for
operation structs. `FileOperation` and `CommandOperation` have required semantic
state and should not gain invalid defaults merely to make construction shorter.

### Migration Documentation

Document the one-time Rust API migration:

Before:

```rust
let span = SourceSpan {
    start: 0,
    end: 0,
    line: 1,
    column: 1,
};
```

After:

```rust
let span = SourceSpan::new(0, 0, 1, 1);
```

Before:

```rust
let Config {
    options,
    files,
    commands,
} = config;
```

After:

```rust
let Config {
    options,
    files,
    commands,
    teardown_commands,
    ..
} = config;
```

Direct field reads remain supported. Explain that future additive fields on
these types are intended to be source-compatible, while removing fields,
changing field types, or changing closed enums can still be breaking.

Update `docs/ARCHITECTURE.md` with a **Public struct evolution** policy next to
the existing public enum policy.

## File-Level Implementation Map

### Core config and schema

- `crates/treeboot-core/src/config.rs`
  - add raw and normalized teardown command collections
  - reuse command normalization helpers
  - apply non-exhaustive attributes
  - share normalized file-operation construction across config, manual, and
    public entry points
  - add stable config-operation constructors
  - add parser and compatibility tests
- `crates/treeboot-core/examples/generate_config_schema.rs`
  - add compact and verbose teardown command declarations
  - reuse the existing command schema types for both phases; do not create
    teardown-specific copies
- `crates/treeboot-core/src/init.rs`
  - add the empty starter teardown section
- `schemas/treeboot.schema.json`
  - regenerate; do not hand-edit
- `crates/treeboot-core/assets/config.schema.json`
  - refresh through the normal generation task

### Planning and execution

- `crates/treeboot-core/src/context.rs`
  - mark `Worktree` non-exhaustive
  - add `Worktree::from_parts` and the shared `Worktree::is_root` predicate
  - replace existing inline root-checkout comparisons
- `crates/treeboot-core/src/validation.rs`
  - add `TeardownPlan`
  - make `PlannedCommands` and command-slice validation shared
  - add complete-config `validate_manifest_phases` aggregation
  - preserve bootstrap `ActionPlan` semantics
- `crates/treeboot-core/src/commands.rs`
  - expose one internal phase-agnostic command runtime
  - retain live cwd revalidation
  - centralize phase-specific event selection
- `crates/treeboot-core/src/teardown.rs`
  - add teardown options, reporter-aware preparation, no-op actions, execution,
    and report types
- `crates/treeboot-core/src/executor.rs`
  - remain the bootstrap file-plus-command orchestrator
  - call the shared command runtime for bootstrap commands
- `crates/treeboot-core/src/output.rs`
  - add teardown command and no-command output events
- `crates/treeboot-core/src/error.rs`
  - add reusable teardown discovery/planning errors
- `crates/treeboot-core/src/lib.rs`
  - export the documented teardown API and plan types

### Inspection commands

- `crates/treeboot-core/src/check.rs`
  - consume shared complete-config phase validation without side effects
  - return a typed aggregate error when either phase fails
- `crates/treeboot-core/src/doctor.rs`
  - map shared phase outcomes to separate diagnostics
- `crates/treeboot/src/commands/config.rs`
  - print teardown commands in text
  - include them through normalized structured output
  - map shared phase outcomes to labelled validation warnings

### CLI adapter

- `crates/treeboot/src/commands/teardown.rs`
  - define Clap arguments
  - map `--worktree` to core cwd
  - implement terminal confirmation through injected/testable I/O
  - execute the already prepared plan
- `crates/treeboot/src/commands/mod.rs`
  - register and dispatch `teardown`
- `crates/treeboot/src/main.rs`
  - use the binary-local error wrapper and preserve existing formatting
- shell completion tests
  - verify the new command and flags appear in generated completions

### Documentation and metadata

- `docs/SPEC.md`
  - bump spec `2.0.0` to `2.1.0`
  - add the fourteenth subcommand
  - define config, prompt, no-op, dry-run, failure, output, and targeting
    behavior
  - define the whole-document normalization and phase-planning boundary
  - include durable prompt and phase-labelled output text
  - define structured config output changes
- `README.md`
  - add teardown to the config overview, safety guidance, examples, and CLI
    table
  - update the referenced spec version
- `crates/treeboot/README.md`
  - keep packaged CLI documentation aligned
- `crates/treeboot-core/README.md`
  - document non-exhaustive construction and teardown embedding
- `docs/ARCHITECTURE.md`
  - add teardown prepare/confirm/execute flow
  - add `TeardownPlan` and module ownership
  - add public struct evolution policy
- `docs/agents/roadmap.md`
  - add a teardown/public-API milestone and mark it implemented only when the
    feature lands
- generated spec metadata
  - refresh with `mise run generate`; never hand-edit
    `crates/treeboot-core/assets/spec-version.txt`

The package version remains release-please controlled. The implementation is a
breaking `treeboot-core` API change and should be released on the next
incompatible `0.x` line, expected to be `0.12.0`, while the independently
versioned behavior specification becomes `2.1.0`.

## Testing Strategy

### Config parsing and normalized output

- Parse omitted teardown declarations as empty.
- Parse compact strings, compact objects, and verbose tables.
- Preserve compact-before-verbose declaration order.
- Apply all existing command defaults.
- Reject `run` plus `program`, `args` without `program`, missing invocation,
  unknown fields, and invalid cwd paths with teardown source locations.
- Confirm setup and teardown commands remain separate in normalized config.
- Confirm JSON and YAML `treeboot config` output includes `teardown_commands`.
- Confirm text output prints an explicit teardown section and `(none)` when
  empty.
- Run the same valid and invalid command-declaration fixtures through config
  parsing and JSON Schema validation so parser/schema drift is detected.
- Confirm the schema accepts every supported teardown form and rejects invalid
  fields without adding duplicate teardown-only schema types.
- Pin the starter config text, confirm it parses, and confirm generated
  artifacts remain fresh.

### Public API compatibility contract

- Add compile-fail doctests proving external crates cannot construct each
  non-exhaustive struct with a literal.
- Add positive public API tests for readable fields and `..` destructuring.
- Replace external-test literals with the new constructors.
- Test `Worktree::from_parts`, readable fields, and `Worktree::is_root`.
- Test `Config::default()` produces an empty valid config.
- Test shell and direct `CommandOperation` constructors and modifiers.
- Test each operation-specific `FileOperation` constructor installs correct
  defaults.
- Test `SourceSpan::new`.
- Extend the source-policy test to require the selected structs to remain
  `#[non_exhaustive]`, using `str::lines()` so CRLF source text remains valid.
- Keep existing exhaustive enum tests unchanged.

### Teardown planning

- Build a plan only from teardown commands.
- Do not include bootstrap files or commands.
- Preserve declaration order.
- Use the worktree root as default cwd.
- Reject planned cwd escapes and Treeboot-owned environment overrides.
- Permit a teardown plan even when bootstrap-only file validation would fail.
- Confirm a bootstrap cwd that normalizes and then escapes does not block
  teardown planning.
- Confirm malformed bootstrap declarations and bootstrap paths that fail during
  `Config::load` block both phases.
- Confirm shared complete-config validation evaluates both phases and preserves
  both errors when both fail.
- Confirm `check` returns the deterministic aggregate phase error.
- Confirm `doctor` and `treeboot config` map the same shared result to
  phase-specific diagnostics and warnings.

### Teardown execution

- Execute teardown commands sequentially.
- Pass the selected worktree's Treeboot environment.
- Honor declared cwd and per-command environment.
- Stop after the first fatal failure.
- Continue after an allowed failure and emit teardown-specific warning output.
- Dry-run every command without spawning.
- Parameterize the shared planner/runtime tests across bootstrap and teardown
  phases. The same `CommandOperation` must yield equivalent planned command
  state and identical cwd, environment, spawn, exit, and allowed-failure
  behavior; only phase event selection differs.
- In those shared tests, cover a cwd created after planning and a cwd symlink
  retargeted outside the worktree after planning.
- Confirm a live cwd rejection never spawns the command, is fatal by default,
  and warns then continues under `allow_failure = true`.
- Confirm no bootstrap file operation or command executes.
- Confirm reporter failures remain typed output errors.
- Confirm preparation emits discovery and no-op events while execution emits
  only command lifecycle events.

### CLI safety and integration

- Run against the current linked worktree.
- Target a linked worktree from the root checkout with `--worktree`.
- Reject the root checkout as the target.
- Confirm run, manual, check, doctor, and teardown all use `Worktree::is_root`,
  while check and doctor retain existing strict/non-strict root behavior.
- Treat missing discovered config as a successful no-op.
- Confirm `TREEBOOT_STRICT` and dangerous file-boundary environment settings do
  not change command-only teardown behavior.
- Fail for an explicitly requested missing config.
- Treat an empty teardown list as a successful no-op without confirmation.
- Refuse non-terminal execution without `--yes`.
- Run non-terminal execution with `--yes`.
- Confirm `--dry-run` needs no terminal and no `--yes`.
- Unit-test prompt parsing for `y`, `yes`, case variants, whitespace, empty
  input, negative input, and EOF through injected readers and writers.
- Unit-test that prompt write or read failures become CLI errors.
- Confirm refusal exits `1`, emits an actionable message, and creates no marker.
- Confirm command failure exits `1`, allowing `&& git worktree remove` to stop.
- Confirm Clap usage errors remain `2`.
- Confirm help and all supported shell completions include `teardown`,
  `--worktree`, `--dry-run`, `--root`, `--config`, and long-only `--yes`.

Prefer platform-neutral unit tests for confirmation logic. Use existing
platform-gated command integration patterns for actual shell execution rather
than adding a pseudo-terminal dependency solely for tests.

### Coverage and full verification

During implementation:

```sh
mise run format
mise run test:core
mise run test:cli
mise run generate
mise run generate:check
```

Before handoff:

```sh
mise run coverage:missing
mise run verify
```

Inspect uncovered lines in the new teardown, confirmation, command-phase, and
constructor code. Add coverage for reachable safety and error branches, but do
not add brittle platform-specific terminal tests where injected I/O gives the
same contract confidence.

## Implementation Sequence

1. Update `docs/SPEC.md` to the complete `2.1.0` contract before coding,
   including the parse/normalization boundary and durable prompt/output text.
2. Apply non-exhaustive attributes, add replacement constructors, add
   `Worktree::is_root`, and consolidate file-operation construction.
3. Update public API tests and documentation so the compatibility break is
   explicit and the workspace is green before adding teardown.
4. Extend config parsing, normalized output, schema markers, and starter config.
5. Add `PlannedCommands`, independent teardown planning, and shared
   complete-config phase validation.
6. Generalize the one command runtime and add centralized teardown-specific
   output event selection.
7. Add the reporter-aware prepare/execute core teardown flow.
8. Add CLI arguments, target selection, confirmation, and CLI-local errors.
9. Update `check`, `doctor`, `config`, completions, README files, architecture,
   and roadmap.
10. Regenerate schema and spec metadata.
11. Run focused tests, inspect missing coverage, then run `mise run verify`.

Keep each step internally green. In particular, land replacement constructors in
the same change that makes their structs non-exhaustive, and do not expose a
teardown CLI that can execute before confirmation behavior and tests exist.

## Acceptance Criteria

- Existing TOML configs behave exactly as before.
- Existing bootstrap file and command output remains unchanged.
- `treeboot teardown` never removes or mutates the Git worktree itself.
- Teardown commands run only after explicit terminal approval or `--yes`.
- Non-interactive execution without `--yes` cannot run a command.
- Refusal and fatal teardown failures return non-zero.
- `--dry-run` reports the complete teardown plan without prompting or spawning.
- Target selection works from inside the worktree and through `--worktree`.
- Whole-document parsing and normalized `Config` construction succeed before
  either phase is planned.
- Bootstrap and teardown semantic planning are independent after that shared
  prerequisite.
- Complete inspection commands consume one shared validation result containing
  both phase outcomes.
- Live cwd boundary enforcement applies immediately before every teardown
  command spawn.
- Config schema, structured config output, starter config, README, spec,
  architecture, and generated metadata agree.
- The selected public config structs are non-exhaustive and have documented
  replacement construction paths.
- `Worktree` is non-exhaustive, has a stable construction replacement, and owns
  the shared root-checkout predicate.
- Bootstrap and teardown share one command planner and runtime; config, manual,
  and public file operations share one normalized construction path.
- `mise run verify` passes.

## Open Questions

None. This plan records the decisions settled in discussion:

- `teardown` terminology
- `teardown_commands` and `[[teardown_command]]`
- command-only teardown
- Treeboot does not remove worktrees
- optional `--worktree` targeting
- terminal approval or long-only `--yes`
- non-zero refusal
- dry-run without approval
- one-time non-exhaustive migration for the normalized public config graph
- whole-document parse/normalization errors are fatal before phase planning
- semantic bootstrap and teardown planning is independent
- one shared command planner/runtime and one complete-config phase validator
- `Worktree` is included in the non-exhaustive migration and owns root detection

# Agent Implementation Guidance

Use this file for implementation tactics while editing code. It is not the
canonical architecture overview. For the current system map, module graph, and
refactor pressure, use [docs/ARCHITECTURE.md](../ARCHITECTURE.md). For
observable behavior, use
[crates/treeboot-spec/SPEC.md](../../crates/treeboot-spec/SPEC.md).

If this file and `docs/ARCHITECTURE.md` disagree about architecture, update this
file or the architecture document so the split is clear. If either file
disagrees with `crates/treeboot-spec/SPEC.md` about behavior, the spec wins
unless the task is explicitly changing the behavior contract.

## What To Use Each Doc For

- `crates/treeboot-spec/SPEC.md`: decide what behavior the code must implement
  and test.
- `docs/ARCHITECTURE.md`: understand current crate/module responsibilities,
  public APIs, command flow, validation/planning/execution flow, reporting, and
  refactor pressure.
- `docs/agents/implementation-guidance.md`: choose implementation placement,
  test layer, and local design constraints while editing.

## Placement Rules

- Put reusable behavior in `crates/treeboot-core`.
- Keep `crates/treeboot` as the CLI adapter: arguments, process-environment
  capture, output formatting, completions, and exit-code mapping.
- Keep public `treeboot-core` APIs documented; the crate denies missing docs.
- Keep `anyhow` out of `treeboot-core`; use typed library errors.
- Keep command-shaped core option defaults environment-pure. CLI or embedder
  process-environment behavior must pass `EnvironmentInput::from_process_env()`
  explicitly.

## Behavioral Constraints

- Git's main worktree is the default root source. Keep Git command behavior
  visible and testable.
- Configured commands are the escape hatch for custom setup; declarative config
  should be validated before side effects.
- File-operation sources are root-anchored. Targets are worktree-anchored.
- Declarative config and manual file commands must share validation and file
  execution semantics.
- Configured commands receive the treeboot environment aliases.

## Testing Cues

- Use core unit tests for pure helpers, validation, planning, file execution,
  command runtime, and output event formatting.
- Use `treeboot-spec` conformance cases for portable user-visible CLI behavior,
  stdout/stderr, exit codes, output formats, filesystem effects, and Git
  linked-worktree behavior.
- Keep `crates/treeboot/tests/` for reference-implementation details excluded
  from the portable suite and the official-binary conformance driver.
- Update `crates/treeboot-spec/SPEC.md` and spec-version metadata when
  observable behavior changes.
- Update `docs/ARCHITECTURE.md` when module boundaries, public core APIs,
  command flow, validation/planning/execution flow, reporting, or refactor
  pressure changes.

Core unit coverage should include config parsing and normalization, duplicate
and outside-boundary target validation, string and object file and command
forms, sync comparison and deletion, teardown command forms and order,
whole-document normalization, independent phase validation, discovery order,
environment construction, conflict policies, relative symlinks, and manual
source-to-target normalization.

For portable conformance cases that depend on Git, create real temporary
repositories and linked worktrees. Exercise those cases from linked worktrees,
including explicit teardown targets and manual operations where relevant, then
assert filesystem effects, command execution, environment values, stdout,
stderr, and exit status. Revalidate a configured command's working directory
immediately before every bootstrap and teardown spawn.

The reference-only integration suite is intentionally narrow: it covers Clap
structure and diagnostic wording, embedded version linkage, direct completion
protocol structure, generated-script markers and Bash/Zsh helper behavior, the
generated Rust schema model, and diagnostic choices that the portable contract
intentionally permits to vary. `crates/treeboot/tests/conformance.rs` runs the
complete portable suite against the official binary.

# Agent Implementation Guidance

Use this file for implementation tactics while editing code. It is not the
canonical architecture overview. For the current system map, module graph, and
refactor pressure, use [docs/ARCHITECTURE.md](../ARCHITECTURE.md). For
observable behavior, use [docs/SPEC.md](../SPEC.md).

If this file and `docs/ARCHITECTURE.md` disagree about architecture, update this
file or the architecture document so the split is clear. If either file
disagrees with `docs/SPEC.md` about behavior, the spec wins unless the task is
explicitly changing the behavior contract.

## What To Use Each Doc For

- `docs/SPEC.md`: decide what behavior the code must implement and test.
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
- Use CLI integration tests for user-visible command behavior, stdout/stderr,
  exit codes, output formats, and Git linked-worktree behavior.
- Update `docs/SPEC.md` and spec-version metadata when observable behavior
  changes.
- Update `docs/ARCHITECTURE.md` when module boundaries, public core APIs,
  command flow, validation/planning/execution flow, reporting, or refactor
  pressure changes.

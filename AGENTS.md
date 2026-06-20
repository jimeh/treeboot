# Agent Guide

## Project Purpose

`treeboot` is a Rust CLI and public core library for bootstrapping Git worktrees
from one repo-local setup contract.

The implementation target is the behavior in [docs/SPEC.html](docs/SPEC.html).
The README is the user-facing summary; the spec is the contract when they differ.

## Spec Discipline

Keep [docs/SPEC.html](docs/SPEC.html) complete enough that a separate
implementation, in another language or runtime, could build a compatible
`treeboot` from the spec alone. When planning uncovers observable behavior,
edge-case semantics, CLI output, validation rules, or compatibility
requirements, update the spec instead of leaving those details only in
implementation plans or roadmap notes. Keep implementation tactics in
`docs/agents/` planning docs.

## Repo Shape

- `crates/treeboot` is the CLI package and should stay thin.
- `crates/treeboot-core` is the public library crate, exposed as
  `treeboot_core`.
- `docs/agents/` contains deeper guidance for future agent work.
- `mise.toml` is the canonical task and tool surface.

Useful deeper docs:

- [docs/agents/architecture.md](docs/agents/architecture.md)
- [docs/agents/validation.md](docs/agents/validation.md)
- [docs/agents/roadmap.md](docs/agents/roadmap.md)
- [docs/agents/dependencies.md](docs/agents/dependencies.md)
- [docs/agents/release.md](docs/agents/release.md)

## Current Implementation State

The current code implements the milestone 1 foundation, milestone 2 config
parsing, milestone 3 declarative validation/planning, milestone 4 config
runtime options, milestone 5 file operations, milestone 6 command runtime, and
milestone 7 shell completions:

- CLI parsing for `run`, `config`, `init`, and `completions`
- Git worktree/root/default-branch discovery
- treeboot environment aliases
- init script discovery and execution
- declarative TOML config parsing and normalization
- declarative TOML validation and run-plan construction
- config/env/CLI runtime option precedence for declarative validation
- view-only normalized config inspection
- generated JSON Schema for the config file format
- starter config/script generation
- static shell completion generation
- structured output events

Declarative TOML config execution currently applies `copy`, `symlink`, and
`sync` file operations, then runs configured commands unless `--skip-commands`
is set. Use `treeboot config` to inspect normalized config without execution;
it warns when run validation would fail.

## Commands

Use `mise` tasks unless a narrower raw Cargo command is clearly better.

```sh
mise run setup      # install tools/deps and hooks
mise run check      # normal pre-handoff confidence
mise run verify     # broad local verification
mise run doctor     # local tool sanity check
mise run coverage   # coverage summary for test-gap work
mise run generate   # refresh checked-in generated artifacts
```

Targeted commands:

```sh
mise run fmt
mise run generate
mise run generate:check
mise run generate:schema:check
mise run lint
mise run test
mise run test:core
mise run test:cli
mise run msrv
mise run actions:lint
mise run coverage:missing
```

See [docs/agents/validation.md](docs/agents/validation.md) for validation tiers
and CI mapping.

## Rust Conventions

- Keep public `treeboot-core` APIs documented; the crate denies missing docs.
- Use typed errors in `treeboot-core`; keep `anyhow` out of the public library.
- Keep `crates/treeboot/src/main.rs` focused on argument parsing, reporting, and
  exit-code mapping.
- Review [docs/agents/dependencies.md](docs/agents/dependencies.md) before
  adding dependencies.
- Prefer borrowing over cloning and avoid `unwrap`/`expect` outside tests.
- Follow existing `rustfmt.toml` width and workspace lint settings.

## Testing Expectations

- Treat tests as part of the implementation, not a follow-up. Do not hand off
  feature work until the new behavior has focused coverage at the right layer.
- For behavior changes, cover the happy path plus edge cases: missing optional
  and required inputs, strict/force/dry-run behavior, conflict handling,
  non-mutation on failure, user-visible output, and platform-specific paths
  when relevant.
- For bug fixes, add a regression test that fails without the fix unless the
  scenario cannot be reproduced in the local harness.
- Use CLI integration tests for user-visible command behavior.
- For run/config CLI behavior inside Git, prefer `git_worktree()` so tests run
  from an actual linked worktree; reserve `git_repo()` for root-checkout cases.
- Use core unit tests for pure helpers, formatting, and validation logic.
- For non-trivial features, run `mise run coverage:missing`, inspect uncovered
  lines in touched modules, and add high-value tests for reachable branches.
  Do not chase brittle coverage for OS permission quirks, platform-only code, or
  defensive I/O error arms unless the behavior is important and testable.
- Put reusable CLI integration helpers in `crates/treeboot/tests/common/`.
- Run `mise run check` before handoff for ordinary code changes.
- Run `mise run verify` for broad harness, CI, release, or architecture changes.

## Harness Notes

- GitHub Actions are pinned and checked with `pinact`.
- Workflow syntax/security checks are wrapped by `mise run actions:lint`.
- Coverage uses `cargo-llvm-cov` through `mise run coverage`; the first run may
  install `llvm-tools-preview` for the active Rust toolchain.
- Pre-commit hooks are managed by Lefthook and installed by `mise run setup`.

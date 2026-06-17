# Agent Guide

## Project Purpose

`treeboot` is a Rust CLI and public core library for bootstrapping Git worktrees
from one repo-local setup contract.

The implementation target is the behavior in [docs/SPEC.html](docs/SPEC.html).
The README is the user-facing summary; the spec is the contract when they differ.

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

The current code implements the milestone 1 foundation and milestone 2 config
parsing:

- CLI parsing for `run`, `config`, and `init`
- Git worktree/root/default-branch discovery
- treeboot environment aliases
- init script discovery and execution
- declarative TOML config parsing and normalization
- view-only normalized config inspection
- generated JSON Schema for the config file format
- starter config/script generation
- structured output events

Declarative TOML config execution is intentionally not implemented yet.
`treeboot run` parses a found config, reports that execution is not implemented,
and exits non-zero. Use `treeboot config` to inspect normalized config without
execution.

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

- Add focused tests for new behavior when the existing suite has a matching
  layer.
- Use CLI integration tests for user-visible command behavior.
- For run/config CLI behavior inside Git, prefer `git_worktree()` so tests run
  from an actual linked worktree; reserve `git_repo()` for root-checkout cases.
- Use core unit tests for pure helpers, formatting, and validation logic.
- Put reusable CLI integration helpers in `crates/treeboot/tests/common/`.
- Run `mise run check` before handoff for ordinary code changes.
- Run `mise run verify` for broad harness, CI, release, or architecture changes.

## Harness Notes

- GitHub Actions are pinned and checked with `pinact`.
- Workflow syntax/security checks are wrapped by `mise run actions:lint`.
- Coverage uses `cargo-llvm-cov` through `mise run coverage`; the first run may
  install `llvm-tools-preview` for the active Rust toolchain.
- Pre-commit hooks are managed by Lefthook and installed by `mise run setup`.

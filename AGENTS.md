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

When changing the observable contract in [docs/SPEC.html](docs/SPEC.html),
bump the visible spec version in that file and keep the README's referenced
spec version in sync.

## Repo Shape

- `crates/treeboot` is the CLI package and should stay thin.
- `crates/treeboot-core` is the public library crate, exposed as
  `treeboot_core`.
- `tools/release-helper` contains release workflow helper logic behind thin
  shell wrappers in `scripts/`.
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
runtime options, milestone 5 file operations, milestone 6 command runtime,
milestone 7 shell completions, milestone 8 manual file operations, and the
first pass of milestone 9 release packaging:

- CLI parsing for `run`, `config`, `init`, `copy`, `symlink`, `sync`, and
  `completions`
- Git worktree/root/default-branch discovery
- treeboot environment aliases
- init script discovery and execution
- declarative TOML config parsing and normalization
- declarative TOML validation and action-plan construction
- config/env/CLI runtime option precedence for declarative validation
- manual root-to-worktree file operation planning and execution
- public Worktree/Manifest/ActionPlan/Executor API surface, with command-shaped
  workflow facades for full treeboot behavior
- view-only normalized config inspection
- generated JSON Schema for the config file format
- starter config/script generation
- shell completion generation with root-relative source completion for manual
  file operations
- release-please version/changelog automation
- tag-triggered and manual release asset packaging
- structured output events

Declarative TOML config execution currently applies `copy`, `symlink`, and
`sync` file operations, then runs configured commands unless `--skip-commands`
is set. Use `treeboot config` to inspect normalized config without execution;
it warns when run validation would fail.

## Commands

Use `mise` tasks unless a narrower raw Cargo command is clearly better.

```sh
mise run setup      # install tools/deps and hooks
mise run check      # normal pre-handoff confidence and generated freshness
mise run verify     # broad local verification
mise run doctor     # local tool sanity check
mise run coverage   # coverage summary for test-gap work
mise run generate   # refresh checked-in generated artifacts
```

Targeted commands:

```sh
mise run format
mise run format:check
mise run generate
mise run generate:check
mise run generate:schema:check
mise run lint
mise run test
mise run test:core
mise run test:cli
mise run test:release-helper
mise run msrv
mise run actions:lint
mise run clean
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
- Mise-managed tools use a 3-day release-age cooldown; use a narrow override
  only for urgent security or CI-maintenance updates.
- `mise run treeboot` is the repo-local bootstrap entrypoint. It keeps the
  released `treeboot` binary task-scoped so CI does not install it as a
  top-level tool, then runs the declarative `.treeboot.toml` setup contract.
- Coverage uses `cargo-llvm-cov` through `mise run coverage`; the first run may
  install `llvm-tools-preview` for the active Rust toolchain.
- Keep optional heavyweight tools task-scoped in `mise.toml`; GitHub Actions
  installs top-level mise tools in every job.
- Pre-commit hooks are managed by Lefthook and installed by `mise run setup`.
- `mise.toml` pins `sccache` and sets `RUSTC_WRAPPER=sccache` so Cargo tasks use
  the project-managed compiler cache instead of relying on global shell setup.
- CI sets `MISE_RUSTUP_HOME` so `mise-action` caches the rustup toolchains and
  components declared in `mise.toml`; cross-OS test jobs use a workspace-local
  path instead of the Ubuntu-only default.
- Release-please must use the repo's `RELEASE_BOT_CLIENT_ID` variable and
  `RELEASE_BOT_PRIVATE_KEY` secret so tags created by release automation trigger
  the tag-based release workflow.
- Android release targets use the hosted runner's Android NDK clang linkers
  instead of `cross`; the cross Android images fail with Rust 1.96 due to
  missing `libunwind` during binary linking.
- Release-please intentionally uses one root Rust release unit without the
  `cargo-workspace` plugin. The root `treeboot-workspace` package exists only
  so release-please can update the root manifest and all workspace member
  versions together while creating the single `vX.Y.Z` product tag. Keep
  `workspace.default-members` aligned with the real build/test packages so the
  inert root package does not replace the normal default Cargo task surface.

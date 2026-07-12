# Agent Guide

## Project Purpose

`treeboot` is a Rust CLI and public core library for bootstrapping Git worktrees
from one repo-local setup contract.

The implementation target is the behavior in [docs/SPEC.md](docs/SPEC.md). The
README is the user-facing summary; the spec is the contract when they differ.

## Spec Discipline

Treat [docs/SPEC.md](docs/SPEC.md) as the source of truth for observable
behavior. If implementation behavior and the spec disagree, fix the
implementation to match the spec unless the task is explicitly changing the
contract. Do not leave drift between code, tests, CLI output, and the spec.

Keep [docs/SPEC.md](docs/SPEC.md) complete enough that a separate
implementation, in another language or runtime, could build a compatible
`treeboot` from the spec alone. When planning uncovers observable behavior,
edge-case semantics, CLI output, validation rules, or compatibility
requirements, update the spec instead of leaving those details only in
implementation plans or roadmap notes. Keep implementation tactics in
`docs/agents/` planning docs.

When changing the observable contract in [docs/SPEC.md](docs/SPEC.md), bump the
visible spec version in that file and keep the README's referenced spec version
in sync.

Before handoff on behavior changes, verify the implementation behavior matches
[docs/SPEC.md](docs/SPEC.md). For changes that affect CLI behavior, config
semantics, validation, filesystem effects, command execution, output, or
compatibility, update the spec in the same change unless it already describes
the final behavior.

## Architecture Discipline

Keep [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) accurate as the current
implementation architecture. Update it when crate/module responsibilities,
public core APIs, command flow, validation/planning/execution flow,
output/reporting architecture, or the documented "Current refactor pressure"
changes.

Use [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the current system map and
[docs/SPEC.md](docs/SPEC.md) for behavioral truth. If those documents appear to
conflict, preserve the spec as the behavior contract and update architecture
wording to describe how the implementation currently satisfies it.

## Pull Request Titles

Pull request titles become changelog entries through release automation. Write
PR titles as concise, user-facing changelog lines, not just branch summaries.
Prefer conventional prefixes when they fit, and make the subject clear when read
in a release note.

## Pull Request Final Review

Add the `coderabbit:review` label only when the PR is fully implemented,
validated, and otherwise ready to merge. Treat the resulting CodeRabbit review
as the final review gate: address its actionable feedback and wait for the gate
to pass before merging.

## Repo Shape

- `crates/treeboot` is the CLI package and should stay thin.
- `crates/treeboot-core` is the public library crate, exposed as
  `treeboot_core`.
- `tools/release-helper` contains release workflow helper logic behind thin
  shell wrappers in `scripts/`.
- `docs/agents/` contains deeper guidance for future agent work.
- `mise.toml` is the canonical task and tool surface.

Useful deeper docs:

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/agents/implementation-guidance.md](docs/agents/implementation-guidance.md)
- [docs/agents/validation.md](docs/agents/validation.md)
- [docs/agents/roadmap.md](docs/agents/roadmap.md)
- [docs/agents/dependencies.md](docs/agents/dependencies.md)
- [docs/agents/release.md](docs/agents/release.md)

## Current Implementation State

The current code implements the milestone 1 foundation, milestone 2 config
parsing, milestone 3 declarative validation/planning, milestone 4 config runtime
options, milestone 5 file operations, milestone 6 command runtime, milestone 7
shell completions, milestone 8 manual file operations, and the first pass of
milestone 9 release packaging, plus milestone 10 inspection and metadata
commands:

- CLI parsing for `run`, `status`, `config`, `check`, `doctor`, `env`, `schema`,
  `version`, `init`, `copy`, `symlink`, `sync`, and `completions`
- Git worktree/root/default-branch discovery
- treeboot environment aliases
- init script discovery and execution
- declarative TOML config parsing and normalization
- declarative TOML validation and action-plan construction
- config/env/CLI runtime option precedence for declarative validation
- manual root-to-worktree file operation planning and execution
- top-level and operation-local copy/sync path ignore rules, including `!`
  re-inclusion
- operation-local copy/sync path include rules with viability pruning, lazy
  directory materialization, and non-fatal zero-match warnings in check/config
- public Worktree/Manifest/ActionPlan/Executor API surface, with command-shaped
  workflow facades for full treeboot behavior
- view-only discovery status inspection
- view-only normalized config inspection
- side-effect-free check, doctor, env, schema, and version inspection commands
- generated JSON Schema for the config file format
- generated spec-version asset and embedded config schema accessors
- starter config/script generation
- shell completion generation with root-relative source completion for manual
  file operations
- release-please version/changelog automation
- tag-triggered and manual release asset packaging
- structured output events

Declarative TOML config execution currently applies `copy`, `symlink`, and
`sync` file operations, then runs configured commands unless `--skip-commands`
is set. Use `treeboot config` to inspect normalized config without execution; it
warns when run validation would fail.

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
mise run format:rust
mise run format:rust:check
mise run format:markdown
mise run format:markdown:check
mise run generate
mise run generate:check
mise run generate:schema:check
mise run harness:check
mise run lint
mise run lint:fix
mise run lint:rust
mise run lint:markdown
mise run test
mise run test:core
mise run test:cli
mise run test:release-helper
mise run release:check
mise run msrv
mise run actions:lint
mise run clean
mise run coverage:missing
```

See [docs/agents/validation.md](docs/agents/validation.md) for validation tiers
and CI mapping.

## Markdown Conventions

Markdown files are formatted with oxfmt and linted with markdownlint-cli2.
`mise run format` and `mise run format:check` include Markdown alongside Rust.
Use `mise run format:markdown` and `mise run lint:markdown` for targeted docs
work. Lefthook checks staged Markdown files through
`mise run lint:markdown:files {staged_files}`.

## Rust Conventions

- Keep public `treeboot-core` APIs documented; the crate denies missing docs.
- Use typed errors in `treeboot-core`; keep `anyhow` out of the public library.
- When a fallible helper spans several inputs (e.g. source vs target file), keep
  it context-agnostic: return a typed error tagged with which input failed, then
  resolve that tag to the path and public `Error` at the caller boundary. If you
  are tempted to thread caller context into a helper only to preserve error
  attribution, treat that as the cue to reach for a tagged error instead.
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
  non-mutation on failure, user-visible output, and platform-specific paths when
  relevant.
- For bug fixes, add a regression test that fails without the fix unless the
  scenario cannot be reproduced in the local harness.
- Use CLI integration tests for user-visible command behavior.
- For run/config CLI behavior inside Git, prefer `git_worktree()` so tests run
  from an actual linked worktree; reserve `git_repo()` for root-checkout cases.
- Use core unit tests for pure helpers, formatting, and validation logic.
- Unit-test chunked or buffered I/O through injected `Read`/`Write` adapters
  (short or staggered reads, `Interrupted`), not just real temp files, and size
  inputs past the internal buffer (8 KiB here) so multi-chunk refill paths run.
- For non-trivial features, run `mise run coverage:missing`, inspect uncovered
  lines in touched modules, and add high-value tests for reachable branches. Do
  not chase brittle coverage for OS permission quirks, platform-only code, or
  defensive I/O error arms unless the behavior is important and testable.
- Put reusable CLI integration helpers in `crates/treeboot/tests/common/`.
- Run `mise run check` before handoff for ordinary code changes.
- Run `mise run verify` for broad harness, CI, release, or architecture changes.

## Harness Notes

- GitHub Actions are pinned and checked with `pinact`.
- Workflow syntax/security checks are wrapped by `mise run actions:lint`.
- Repo harness invariants are wrapped by `mise run harness:check`; keep
  dependency-boundary and spec-version drift checks there when they can be
  expressed without heavyweight tooling.
- Do not require package-version literals in `docs/SPEC.md` examples to match
  Cargo package versions. Release-please does not update spec examples, and
  example version drift should not block release PRs.
- Dependabot Cargo and GitHub Actions version updates use a 7-day cooldown.
  Security updates are not affected by Dependabot cooldown and should stay
  alert-driven.
- Renovate is scoped to monthly mise lockfile and Rust toolchain maintenance. It
  runs from `.github/workflows/renovate-mise.yml` with the release bot GitHub
  App token and uses `.github/renovate-mise.config.js` as self-hosted/global
  config. Keep ordinary mise dependency updates disabled: humans own the
  constraints in `mise.toml`, while lockfile maintenance refreshes `mise.lock`
  within them. Keep `allowedUnsafeExecutions = ["mise"]` for mise lockfile
  refreshes and exact allowlist entries for `mise trust mise.toml` and
  `mise lock rust` so Rust toolchain PRs can trust the temporary checkout before
  updating `mise.lock` with `rust-toolchain.toml`. Keep that package-rule task
  in `executionMode = "update"`; branch mode skips the task. Manual dispatch
  sets `RENOVATE_BYPASS_SCHEDULE` so emergency runs bypass the internal Renovate
  schedule as well as the GitHub Actions cron gate, and exposes an
  `info`/`debug` Renovate log-level choice for troubleshooting. Scheduled runs
  default to `info` and make three attempts within the monthly update window so
  a concurrent default-branch change does not delay maintenance for a month.
  Keep `:disableDependencyDashboard` in the Renovate preset list; with
  `config:recommended`, `dependencyDashboard: false` alone can still produce a
  Dependency Dashboard issue in this self-hosted flow. Renovate PR creation is
  intentionally `immediate` so updates behave like Dependabot updates. The
  release bot token also needs commit-status write permission; otherwise
  Renovate aborts while setting `renovate/stability-days` and reports the
  misleading `repository-changed` branch error.
- Mise-managed tools use a 7-day release-age cooldown and checked-in
  `mise.lock`; use a narrow override only for urgent security or CI-maintenance
  updates.
- `mise run treeboot` is the repo-local bootstrap entrypoint. The released
  `treeboot` binary is a project-wide mise tool so it is available to direct
  commands and other tasks; the task runs the declarative `.treeboot.toml` setup
  contract.
- Coverage uses `cargo-llvm-cov` through `mise run coverage`; the first run may
  install `llvm-tools-preview` for the active Rust toolchain.
- Keep optional heavyweight tools task-scoped in `mise.toml`; GitHub Actions
  installs top-level mise tools in every job.
- Keep `settings.lockfile_platforms` aligned with GitHub Actions host runner
  platforms. Release target triples such as Android or musl do not need lockfile
  platforms unless `mise install --locked` runs on that host OS/architecture.
- Pre-commit hooks are managed by Lefthook and installed by `mise run setup`.
- `mise.toml` pins `sccache` and sets `RUSTC_WRAPPER=sccache` so Cargo tasks use
  the project-managed compiler cache instead of relying on global shell setup.
- Rust toolchain version and components live in `rust-toolchain.toml` so Rustup
  and mise consume the same source. Mise exports that version through
  `RUSTUP_TOOLCHAIN`; CI install steps rely on it instead of duplicating the
  version in workflow YAML. Renovate updates the toolchain and runs
  `mise lock rust` in the same branch so locked installs remain usable.
- CI sets `MISE_RUSTUP_HOME` so `mise-action` caches the rustup toolchains and
  components declared by the project; cross-OS test jobs use a workspace-local
  path instead of the Ubuntu-only default.
- CI test jobs install the configured Rust toolchain in one serial step before
  `mise run test`; the aggregate test task uses one Cargo invocation so shared
  test-profile compilation is not split across parallel package tasks.
- CI runs Rust linting on both Ubuntu and Windows so platform-gated code is
  checked with warnings denied.
- Release-please and Renovate must use the repo's `RELEASE_BOT_CLIENT_ID`
  variable and `RELEASE_BOT_PRIVATE_KEY` secret so automation-created commits
  and PRs trigger the expected follow-up workflows.
- Android release targets use the hosted runner's Android NDK clang linkers
  instead of `cross`; the cross Android images fail with Rust 1.96 due to
  missing `libunwind` during binary linking.
- Android release asset names intentionally omit the Rust target triple's
  `linux` segment (`x86_64-android`, not `x86_64-linux-android`) so desktop
  Linux GitHub release installers such as mise do not pick Android archives.
- Release-please intentionally uses one root Rust release unit without the
  `cargo-workspace` plugin. The root `treeboot-workspace` package exists only so
  release-please can update the root manifest and all workspace member versions
  together while creating the single `vX.Y.Z` product tag. Keep
  `workspace.default-members` aligned with the real build/test packages so the
  inert root package does not replace the normal default Cargo task surface.
- For crates.io publishing, keep `treeboot`'s dependency on `treeboot-core` as
  both `path = "../treeboot-core"` and the matching registry `version`; Cargo
  rejects publishable packages with path-only normal dependencies. Member crates
  need crate-local READMEs or explicit readme metadata, otherwise Cargo packages
  them with `readme = false`. Keep the crate-local `LICENSE` copies in sync with
  the root `LICENSE` so published crate tarballs include the license text.
- crates.io Trusted Publishing is bound to `.github/workflows/release.yml` and
  the GitHub Actions `release` environment for both published crates. Keep the
  crates.io Trusted Publisher settings in sync if either name changes.

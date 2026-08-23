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

Keep implementation plans focused on sequencing, placement, and behavior/test
closure. Link to settled behavior in [docs/SPEC.md](docs/SPEC.md) and current
architecture in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) instead of
duplicating either contract in a planning document.

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

After implementation, dual review, final-head CI, and local handback pass, mark
the PR ready and then add the `coderabbit:review` label. A CodeRabbit status
reported while the PR is still draft does not satisfy the gate. Require the
resulting review or check to cover the exact final head.

Treat CodeRabbit as the final merge gate: address its actionable feedback and
wait for the gate to pass before merging. If its findings require changes,
return the PR to draft while correcting them, then repeat affected validation
and final-head review.

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
  `version`, `init`, `copy`, `symlink`, `sync`, `completions`, and the
  `worktree id`, `worktree slug`, `worktree path`, and `worktree list`
  inspection commands
- Git worktree/root/default-branch discovery
- treeboot environment aliases
- stable config-refined `TREEBOOT_WORKTREE_ID` and `TREEBOOT_WORKTREE_SLUG`
  command environments
- declarative TOML config parsing and normalization
- declarative TOML validation and action-plan construction
- config/env/CLI runtime option precedence for declarative validation
- manual root-to-worktree file operation planning and execution
- top-level and operation-local copy/sync path ignore rules, including `!`
  re-inclusion
- operation-local copy/sync path include rules with viability pruning, lazy
  directory materialization, and non-fatal zero-match warnings in check/config
- public Worktree/Manifest/ActionPlan/Executor and worktree-inspection API
  surfaces, with command-shaped workflow facades for full treeboot behavior
- view-only discovery status inspection
- view-only normalized config inspection
- side-effect-free check, doctor, config-aware env, exact-target worktree
  ID/slug, repository worktree path/list, schema, and version inspection
  commands
- generated JSON Schema for the config file format
- generated spec-version asset and embedded config schema accessors
- starter config generation
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
mise run audit:deps
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
- Manual file-operation construction intentionally preserves lexical context
  path identity. Do not canonicalize root or worktree aliases there; config and
  standalone public constructors retain their existing-aware normalization.
- Root-target checks use Git's discovered main-worktree identity, not the
  overridable file-operation source root. Shared planned commands retain their
  planning-time canonical worktree boundary and recheck it before every spawn.
- Keep `crates/treeboot/src/main.rs` focused on argument parsing, reporting, and
  exit-code mapping.
- Review [docs/agents/dependencies.md](docs/agents/dependencies.md) before
  adding dependencies.
- Prefer borrowing over cloning and avoid `unwrap`/`expect` outside tests.
- Follow existing `rustfmt.toml` width and workspace lint settings.

## Testing Expectations

- Treat tests as part of the implementation, not a follow-up. Do not hand off
  feature work until the new behavior has focused coverage at the right layer.
- For non-trivial behavior changes, treat the frozen plan's behavior/test matrix
  as a completion checklist. Before the initial push, map every specified happy,
  failure, boundary, regression, and platform case to a named test or explicit
  non-goal.
- For behavior changes, cover the happy path plus edge cases: missing optional
  and required inputs, strict/force/dry-run behavior, conflict handling,
  non-mutation on failure, user-visible output, and platform-specific paths when
  relevant.
- For inspection and reporting commands, cover text, JSON, and YAML output
  parity. Structured serialization failures must occur before stdout is written
  so automation never receives a partial document.
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
- Git on macOS rejects non-UTF-8 worktree administrative directory names with
  `Illegal byte sequence`; keep filesystem-backed non-UTF-8 worktree fixtures
  Linux-gated while retaining platform-independent native-path coverage.
- Put reusable CLI integration helpers in `crates/treeboot/tests/common/`.
- Use affected targeted tasks during implementation and correction rounds; see
  the validation guide for correction and cross-platform preflight rules.
- Run `mise run check` on the intended handoff head for ordinary code changes.
- Run `mise run verify` on the intended final local head for broad harness, CI,
  release, or architecture changes. Rerun it only when later changes invalidate
  that broad evidence.

## Harness Notes

- GitHub Actions are pinned and checked with `pinact`.
- Workflow syntax/security checks are wrapped by `mise run actions:lint`.
- Rust dependencies are checked against RustSec by `mise run audit:deps`.
- Repo harness invariants are wrapped by `mise run harness:check`; keep
  dependency-boundary and spec-version drift checks there when they can be
  expressed without heavyweight tooling.
- Do not require package-version literals in `docs/SPEC.md` examples to match
  Cargo package versions. Release-please does not update spec examples, and
  example version drift should not block release PRs.
- Dependabot Cargo and GitHub Actions version updates use a 7-day cooldown.
  Security updates are not affected by Dependabot cooldown and should stay
  alert-driven.
- Renovate is scoped to monthly mise tool, lockfile, and Rust toolchain
  maintenance. It runs from `.github/workflows/renovate-mise.yml` with the
  release bot GitHub App token and uses `.github/renovate-mise.config.js` as
  self-hosted/global config. Mise tools use major-version constraints in
  `mise.toml`, while `mise.lock` records the exact resolved versions. Keep the
  `github:jimeh/treeboot` `extractVersion` rule so GitHub release tags do not
  reintroduce a `v` prefix into `mise.toml`. Keep
  `allowedUnsafeExecutions = ["mise"]` for mise lockfile refreshes. Keep exact
  allowlist entries for `mise trust mise.toml` and `mise lock rust` so Rust
  toolchain PRs can trust the temporary checkout before updating `mise.lock`
  with `rust-toolchain.toml`. Keep that package-rule task in
  `executionMode = "update"`; branch mode skips the task. Manual dispatch sets
  `RENOVATE_BYPASS_SCHEDULE` so emergency runs bypass the internal Renovate
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
- Give task-scoped Cargo tools concrete versions such as `0.22.2`; an `=0.22.2`
  requirement makes Mise warn that the semver range is unsupported.
- Leave `settings.lockfile_platforms` unset. Dev-tool artifact coverage is
  independent of the GitHub Actions hosts and release targets the project builds
  and tests.
- Pre-commit hooks are managed by Lefthook and installed by `mise run setup`.
- `mise.toml` manages `sccache` and sets `RUSTC_WRAPPER=sccache` so Cargo tasks
  use the project-managed compiler cache instead of relying on global shell
  setup.
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
- Musl release smoke tests run the `cli` integration test under `cross` instead
  of forwarding `--version` through `cross run`. Cross 0.2.5 drops the binary
  arguments in this path, causing treeboot's default command to run against the
  image's unsupported Git 2.25.1.
- When validating multiple `cross` targets locally, give each target a distinct
  `CARGO_TARGET_DIR`. Reused host build scripts can require newer glibc than the
  Ubuntu 20.04 cross images provide.
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

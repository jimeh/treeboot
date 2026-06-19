# Implementation Roadmap

This roadmap keeps future implementation work aligned with
[docs/SPEC.html](../SPEC.html). Update it when a milestone is completed or the
spec changes.

## Milestone 1: Run Context And Discovery

Status: implemented.

Scope:

- two-crate workspace with `treeboot` CLI and public `treeboot-core`
- `treeboot` equals `treeboot run`
- `run` and `init` CLI surfaces
- Git worktree/root/default-branch discovery
- treeboot environment aliases
- init script discovery and execution
- config discovery with explicit not-yet-implemented handling
- structured output events

## Milestone 2: Config Parsing And Normalization

Status: implemented.

Scope:

- parse `.treeboot.toml`, `treeboot.toml`, and explicit `--config`
- inspect normalized config with `treeboot config`
- support `copy`, `symlink`, `sync`, `files`, `[[file]]`
- support `commands` and `[[command]]`
- normalize declarations into ordered file and command operation models
- preserve enough source context for useful validation errors

Validation focus:

- string and object forms
- declaration ordering
- unknown/invalid fields
- mutually exclusive command fields
- clear parse errors

## Milestone 3: Declarative Validation

Status: implemented.

Scope:

- duplicate configured target detection
- target boundary validation
- source boundary validation
- command `cwd` boundary validation
- owned environment variable override rejection
- strict-mode sync rejection
- unsafe source symlink detection for copy/sync

Validation must complete before file operations or configured commands run.

## Milestone 4: Config Runtime Options

Status: implemented.

Scope:

- parse top-level `strict`
- move `dangerously_allow_sources_outside_root` and
  `dangerously_allow_targets_outside_worktree` to the config top level
- remove the nested `[validation]` config table from the spec model
- support `TREEBOOT_STRICT`,
  `TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT`, and
  `TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE`
- apply precedence: defaults, then config, then environment, then CLI flags
- reject invalid boolean environment values before side effects
- update generated JSON Schema and starter config output

Validation focus:

- top-level option parsing
- unknown nested `[validation]` rejection
- environment override parsing and precedence
- strict behavior before and after config discovery

## Milestone 5: File Operations

Status: pending.

Scope:

- copy files and directories
- create relative symlinks when possible
- sync files/directories with metadata and checksum comparison modes
- implement default, strict, force, and dry-run conflict behavior
- preserve safe source symlinks for copy/sync

Validation focus:

- idempotent default behavior
- force behavior around files, symlinks, and directories
- sync explicit delete behavior
- unsafe symlink rejection before side effects

## Milestone 6: Command Runtime

Status: pending.

Scope:

- run shell commands and direct program/args commands
- apply treeboot environment plus per-command env
- support command `cwd`
- implement `allow_failure`
- implement async command batching
- support `--skip-commands` and `--dry-run`

Async implementation should be decided here. Keep milestone 1 synchronous until
command batching needs concurrency.

## Milestone 7: Shell Completion

Status: pending.

Scope:

- add a built-in `treeboot completions <shell>` command
- use the clap completion ecosystem, such as `clap_complete`, to generate
  scripts from the same CLI definition used for runtime parsing
- support Bash, Zsh, Fish, PowerShell, and Elvish when available from the
  generator crate
- keep completion generation side-effect free by writing scripts to stdout
- include completion installation notes in release or install documentation
- add the completion plumbing needed for command-specific dynamic candidates

Validation focus:

- every supported shell value emits non-empty script output
- unsupported shell values fail with a CLI usage error
- generated scripts include implemented subcommands and options
- completion plumbing can be extended by later commands for dynamic candidates

## Milestone 8: Manual File Operation Commands

Status: pending.

Scope:

- add `treeboot copy`, `treeboot symlink`, and `treeboot sync`
- reuse the same normalized file operation model as declarative config
- require one or more source arguments
- support root-path-based shell completion for source arguments
- support `--target` for one source and as a path prefix for multiple sources
- expose relevant operation flags: `--required`, `--symlinks`, `--compare`,
  `--delete`, and `--no-delete`
- support shared `--root`, `--strict`, `--force`, and `--dry-run` behavior
- skip config discovery, init script discovery, and configured commands

Validation focus:

- source-to-target normalization for one and many sources
- duplicate target rejection before side effects
- source and target boundary checks
- operation-specific flag validation
- completion candidates come from the resolved root path

## Milestone 9: Release Packaging

Status: pending.

Scope:

- release target matrix from the spec
- archive and raw executable assets
- checksums and checksum signature
- SBOM and provenance artifacts
- macOS signing/notarization notes
- release smoke tests

See [release.md](release.md) before implementing packaging automation.

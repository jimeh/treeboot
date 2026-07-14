# Implementation Roadmap

This roadmap keeps future implementation work aligned with
[docs/SPEC.md](../SPEC.md). Update it when a milestone is completed or the spec
changes.

## Milestone 1: Run Context And Discovery

Status: implemented.

Scope:

- two-crate workspace with `treeboot` CLI and public `treeboot-core`
- `treeboot` equals `treeboot run`
- `run` and `init` CLI surfaces
- Git worktree/root/default-branch discovery
- treeboot environment aliases
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
- support `TREEBOOT_STRICT`, `TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT`,
  and `TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE`
- apply precedence: defaults, then config, then environment, then CLI flags
- reject invalid boolean environment values before side effects
- update generated JSON Schema and starter config output

Validation focus:

- top-level option parsing
- unknown nested `[validation]` rejection
- environment override parsing and precedence
- strict behavior before and after config discovery

## Milestone 5: File Operations

Status: implemented.

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

Status: implemented.

Scope:

- run shell commands and direct program/args commands
- apply treeboot environment plus per-command env
- support command `cwd`
- implement `allow_failure`
- run configured commands sequentially in declaration order
- support `--skip-commands` and `--dry-run`

## Milestone 7: Shell Completion

Status: implemented.

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

Status: implemented.

Scope:

- add `treeboot copy`, `treeboot symlink`, and `treeboot sync`
- reuse the same normalized file operation model as declarative config
- require one or more source arguments
- support root-path-based shell completion for source arguments
- support `--target` for one source and as a path prefix for multiple sources
- expose relevant operation flags: `--required`, `--symlinks`, `--compare`,
  `--delete`, and `--no-delete`
- support shared `--root`, `--strict`, `--force`, and `--dry-run` behavior
- skip configured commands and actions while loading config policy when present

Validation focus:

- source-to-target normalization for one and many sources
- duplicate target rejection before side effects
- source and target boundary checks
- operation-specific flag validation
- completion candidates come from the resolved root path

## Milestone 9: Release Packaging

Status: first pass implemented; signing hardening pending.

Scope:

- release target matrix from the spec
- archive and raw executable assets
- checksums
- SBOM and provenance artifacts
- macOS signing/notarization notes
- release smoke tests

See [release.md](release.md) before implementing packaging automation.

## Milestone 10: Inspection And Metadata Commands

Status: implemented.

Scope:

- add `treeboot version` alongside the existing `-V`/`--version` flags
- embed the implemented spec version and generated config schema in
  `treeboot-core`
- add `treeboot schema` with stdout and `--output/-o` support
- add `treeboot check` for side-effect-free run validation
- add `treeboot doctor` for structured diagnostics
- add `treeboot env` for the environment exposed to configured commands
- support `--format text|json|yaml`, `--json`, and `--yaml` for `status`,
  `config`, `version`, `check`, `doctor`, and `env`
- expose core crate report functions for embedders that need command-shaped
  behavior without reimplementing CLI wiring

Validation focus:

- text output stays human-oriented and useful by default
- JSON and YAML outputs are parseable and stable enough for automation
- check and doctor perform no file or configured command side effects
- generated schema and metadata freshness is enforced by
  `mise run generate:check`

## Milestone 11: Path Include Rules

Status: implemented.

Scope:

- `include` option on copy and sync operations in declarative config and the
  manual `treeboot copy` / `treeboot sync` commands
- include and ignore as independent gates during directory-source traversal
- include entries restricted to effective positive patterns; `!` negation, blank
  entries, and `#` comments are validation errors
- lazy directory materialization keyed on included descendants
- conservative viability pruning of directories that cannot contain matches
- rejection of non-empty `include` combined with sync `delete = true`
- non-fatal zero-match include warnings through a new `ActionPlan` warnings
  channel, surfaced by `treeboot check` (report field) and `treeboot config`
  (stderr), silent in `run`

Validation focus:

- include gate semantics at parse, manual-option, plan, and CLI layers
- pruned directories are never read
- ancestor metadata repair with unchanged included descendants
- warning output shapes across text, JSON, and YAML

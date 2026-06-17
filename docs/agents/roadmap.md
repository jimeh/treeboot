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

Status: pending.

Scope:

- duplicate configured target detection
- target boundary validation
- source boundary validation
- command `cwd` boundary validation
- owned environment variable override rejection
- strict-mode sync rejection
- unsafe source symlink detection for copy/sync

Validation must complete before file operations or configured commands run.

## Milestone 4: File Operations

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
- sync delete-extra behavior
- unsafe symlink rejection before side effects

## Milestone 5: Command Runtime

Status: pending.

Scope:

- run shell commands and direct program/args commands
- apply treeboot environment plus per-command env
- support command `cwd`
- implement `allow_failure`
- implement async command batching
- support `--no-commands` and `--dry-run`

Async implementation should be decided here. Keep milestone 1 synchronous until
command batching needs concurrency.

## Milestone 6: Release Packaging

Status: pending.

Scope:

- release target matrix from the spec
- archive and raw executable assets
- checksums and checksum signature
- SBOM and provenance artifacts
- macOS signing/notarization notes
- release smoke tests

See [release.md](release.md) before implementing packaging automation.

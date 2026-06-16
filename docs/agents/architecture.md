# Architecture Notes

## Crate Boundaries

`treeboot` is split into two packages:

- `treeboot`: binary CLI package
- `treeboot-core`: public library package, imported as `treeboot_core`

The CLI crate should remain a small adapter over the library. It owns:

- `clap` argument definitions
- stdout/stderr presentation
- process exit-code mapping

The core crate owns reusable behavior:

- runtime context discovery
- Git command wrappers
- script and config discovery
- init file generation
- structured output events
- typed errors

## Core Modules

- `context.rs`: resolves worktree path, root path, default branch, and env vars
- `git.rs`: wraps Git CLI calls
- `discovery.rs`: finds init scripts and config files
- `run.rs`: orchestrates `treeboot run`
- `init.rs`: implements `treeboot init`
- `output.rs`: defines output events and reporter abstraction
- `error.rs`: typed library errors and exit-code mapping

## Design Constraints

- The spec uses Git's main worktree as the default root source; keep `git`
  command behavior visible and testable.
- Init scripts are unrestricted escape hatches. Declarative config should be
  validated before side effects.
- File-operation targets are worktree-anchored. Sources are root-anchored.
- Commands and scripts receive the same treeboot environment aliases.
- Future config/file/command execution should extend `treeboot-core`; the CLI
  should only expose options and reporting.

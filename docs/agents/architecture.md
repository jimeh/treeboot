# Architecture Notes

## Crate Boundaries

`treeboot` is split into two packages:

- `treeboot`: binary CLI package
- `treeboot-core`: public library package, imported as `treeboot_core`

The CLI crate should remain a small adapter over the library. It owns:

- `clap` argument definitions
- process-environment capture for CLI compatibility variables
- text, JSON, and YAML presentation for inspection commands
- stdout/stderr presentation for structured output events and errors
- shell completion generation and completion integration
- process exit-code mapping

The core crate owns reusable behavior:

- runtime context discovery
- Git command wrappers
- script and config discovery
- declarative config parsing and normalization
- declarative validation and action-plan construction
- file-operation planning and execution
- configured command execution
- command-shaped workflow facades for run, status, config, check, doctor, env,
  schema metadata, version metadata, init, and manual file operations
- init file generation
- generated metadata/schema assets for config authoring and version reporting
- structured output events
- typed errors

## Core Modules

- `check.rs`: validates selected bootstrap behavior without side effects
- `commands.rs`: runs configured commands from validated action plans
- `context.rs`: resolves worktree path, root path, default branch, and env vars
- `git.rs`: wraps Git CLI calls
- `discovery.rs`: finds init scripts and config files
- `config.rs`: parses and normalizes declarative TOML config
- `doctor.rs`: builds discovery and validation diagnostics
- `env.rs`: inspects child environment variables for scripts and commands
- `executor.rs`: applies validated action plans
- `files.rs`: applies copy, symlink, and sync file operations
- `examples/generate_config_schema.rs`: generates `schemas/treeboot.schema.json`
- `metadata.rs`: exposes embedded schema and version metadata
- `manual.rs`: maps manual `copy`, `symlink`, and `sync` commands to action plans
- `run.rs`: orchestrates `treeboot run`
- `status.rs`: inspects worktree, config, and init-script discovery state
- `validation.rs`: converts normalized config or manual specs into action plans
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
- `treeboot-core` command-shaped option defaults are environment-pure. When the
  CLI or an embedder wants process-environment compatibility behavior, it must
  pass `EnvironmentInput::from_process_env()` explicitly.
- New reusable behavior should extend `treeboot-core`; the CLI should only
  expose arguments, output formatting, completions, and process handling.

# treeboot

Bootstrap new Git worktrees from one repo-local setup file.

`treeboot` is meant for teams and agents that create lots of Git worktrees. A
new worktree often needs the same local setup every time: copy an `.env`, link
shared tooling, install dependencies, or run a project setup command.

Instead of repeating those steps across Codex, Claude Code, Conductor,
Superset, shell scripts, and team docs, put them in one place and run:

```sh
treeboot
```

## Status

This project is bootstrapped for implementation against spec v1.1.0. The
planned implementation target is Rust, distributed as small prebuilt binaries
from GitHub Releases.

The initial implementation contract lives in
[docs/SPEC.html](./docs/SPEC.html). This README is the short human-facing
version.

The current implementation is in progress. It supports the initial `run`,
`config`, and `init` command surfaces, path discovery, init-script
discovery/execution, config parsing/inspection, and missing-config behavior.
Declarative TOML config execution is still pending.

## Why

Git worktrees are cheap to create, but project-local state usually is not.
Most real projects have files and commands that are intentionally not committed:

- `.env` and `.env.local`
- local agent or editor config
- shared scripts
- language runtime installs
- dependency installation commands

`treeboot` gives those setup steps a single home.

## Quick Start

Create `.treeboot.toml` in the root checkout:

```toml
copy = [
  ".env",
  ".env.local",
]

symlink = [
  ".tool-versions",
  { source = "shared/.agents", target = ".agents" },
]

commands = [
  "mise install",
  { name = "Install Ruby gems", run = "bundle install", async = true },
  { name = "Install Node packages", run = "pnpm install", async = true },
]
```

Then run from a new worktree:

```sh
treeboot
```

By default, copy and symlink operations are idempotent. If a target already
exists, `treeboot` reports it and leaves it alone. Commands still run, so write
commands to be safe to rerun.

## Config

The default config file is:

```text
.treeboot.toml
```

The common top-level config keys are:

```toml
strict = true
dangerously_allow_sources_outside_root = false
dangerously_allow_targets_outside_worktree = false

copy = [
  ".env",
  { source = "templates/local.env", target = ".env.local" },
]

symlink = [
  ".tool-versions",
  { source = "shared/bin", target = "bin" },
]

sync = [
  { source = "shared/editor", target = ".editor" },
]

commands = [
  "mise install",
  { run = "bundle install", async = true },
  { run = "pnpm install", async = true },
]
```

String file entries use the same source and target path. Object entries also
default `target` to `source` when only `source` is set, and can set a different
target when needed. Missing sources are skipped by default; set
`required = true` on a file object when a missing source should fail.

Use `sync` when the target should be actively reconciled with the source.
Directory sync deletes target-only files by default, so it is intentionally more
destructive than `copy`.

## Scripts

For cases where config is not enough, use an executable init script:

```text
.treeboot.sh
```

If an executable init script exists, `treeboot` runs it instead of declarative
config. The script runs from the worktree root and receives the root checkout
path as its first argument:

```sh
#!/usr/bin/env sh
set -eu

root_path="$1"

ln -s "$root_path/.env" .env
mise install
```

## Commands

`treeboot` and `treeboot run` are equivalent:

```sh
treeboot
treeboot run
treeboot config
treeboot config --format json
```

Useful planned options:

```sh
treeboot run --dry-run
treeboot run --strict
treeboot run --force
treeboot run --root /path/to/root-checkout
treeboot init
```

`treeboot init` creates a starter config or script. In an interactive shell it
prompts for which one to create.

## Safety

`treeboot` is conservative by default:

- existing copy and symlink targets are skipped
- duplicate configured targets are config errors
- file targets must stay inside the current worktree
- `--strict` fails on existing copy/symlink targets and rejects sync operations
- `--force` is the explicit mode for replacing existing targets

If no config or executable init script is found, `treeboot` prints an info
message and exits successfully. With `--strict`, that same case exits non-zero.

## Environment

Init scripts and configured commands receive the same environment. The canonical
variables are:

```sh
TREEBOOT_ROOT_PATH
TREEBOOT_WORKTREE_PATH
TREEBOOT_DEFAULT_BRANCH
```

`treeboot` also sets compatibility aliases for common agent setup scripts,
including Codex, Conductor, Superset, and generic Git-style names. See
[docs/SPEC.html](./docs/SPEC.html) for the full mapping.

## Schema

The checked-in JSON Schema for `.treeboot.toml` is generated at:

```text
schemas/treeboot.schema.json
```

Regenerate it with:

```sh
mise run generate
```

## Install

The intended install path is downloading the release asset for your platform
from GitHub Releases, either directly or through tools such as `ubi` and
`mise`.

Releases are expected to include archives, raw executable assets, checksums,
a signed checksum manifest, SBOMs, attestations, and signed/notarized macOS
binaries.

## Name

`treeboot` means "worktree bootstrap".

## Development

This project uses `mise` for runtime/tool management and task running:

```sh
mise run setup
mise run check
mise run verify
mise run ci
```

Useful individual tasks:

```sh
mise run actions:lint
mise run build
mise run build:release
mise run coverage
mise run coverage:missing
mise run deps
mise run doctor
mise run fmt
mise run generate
mise run generate:check
mise run generate:schema:check
mise run hooks:install
mise run lint
mise run msrv
mise run test
mise run test:core
mise run test:cli
```

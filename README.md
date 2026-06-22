<div align="center">

<img width="196px" src="./img/treeboot.svg" alt="Logo">

# treeboot

**Bootstrap new Git worktrees from one repo-local setup file.**

[![GitHub Release](https://img.shields.io/github/v/release/jimeh/treeboot?logo=github&label=Release)](https://github.com/jimeh/treeboot/releases/latest)
[![crates.io](https://img.shields.io/crates/v/treeboot?logo=rust&label=crates.io)](https://crates.io/crates/treeboot)
[![docs.rs](https://img.shields.io/docsrs/treeboot-core?logo=docs.rs&label=docs.rs)](https://docs.rs/treeboot-core)
[![GitHub Issues](https://img.shields.io/github/issues/jimeh/treeboot?logo=github&label=Issues)](https://github.com/jimeh/treeboot/issues)
[![GitHub Pull Requests](https://img.shields.io/github/issues-pr/jimeh/treeboot?logo=github&label=PRs)](https://github.com/jimeh/treeboot/pulls)
[![License](https://img.shields.io/github/license/jimeh/treeboot?label=License)](https://github.com/jimeh/treeboot/blob/main/LICENSE)

</div>

`treeboot` is meant for teams and agents that create lots of Git worktrees. A
new worktree often needs the same local setup every time: copy an `.env`, link
shared tooling, install dependencies, or run a project setup command.

Instead of repeating those steps across Codex, Claude Code, Conductor,
Superset, shell scripts, and team docs, put them in one place and run:

```sh
treeboot
```

## Status

This project is bootstrapped for implementation against spec v1.7.1. The
planned implementation target is Rust, distributed as small prebuilt binaries
from GitHub Releases.

The initial implementation contract lives in
[docs/SPEC.html](./docs/SPEC.html). This README is the short human-facing
version.

The current implementation is in progress. It supports the `run`, `config`,
`init`, `copy`, `symlink`, `sync`, and `completions` command surfaces, path
discovery, init-script discovery/execution, config parsing/inspection,
declarative validation, declarative file operations, declarative command
execution, shell completion generation, root-relative manual source
completion, and missing-config behavior.

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
  { name = "Install dependencies", run = "mise run setup" },
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
  { run = "mise run setup" },
]
```

String file entries use the same source and target path. Object entries also
default `target` to `source` when only `source` is set, and can set a different
target when needed. Missing sources are skipped by default; set
`required = true` on a file object when a missing source should fail.

Use `sync` when the target should be actively reconciled with the source.
Directory sync preserves target-only files by default. Set `delete = true` when
target-only files and directories should be removed.

Commands run sequentially in declaration order. If setup work should run in
parallel, put that behind one project-local task-runner command such as
`mise run setup`.

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

Use `treeboot run --no-init-script` to ignore executable init scripts and use
normal config discovery instead.

## Commands

`treeboot` and `treeboot run` are equivalent:

```sh
treeboot
treeboot run
treeboot config
treeboot config --format json
treeboot config --json
treeboot copy .env
treeboot symlink .tool-versions
treeboot sync shared/config --compare checksum
treeboot completions bash
```

Useful options:

```sh
treeboot run --dry-run
treeboot run --strict
treeboot run --force
treeboot run --root /path/to/root-checkout
treeboot run --no-init-script
treeboot copy .env .npmrc --target local
treeboot sync shared/config --delete --dry-run
treeboot init
```

`treeboot init` creates `.treeboot.toml` by default. Use
`treeboot init --script` to create `.treeboot.sh` instead. Existing init
targets, including symlinks, are never replaced.

## Shell Completions

`treeboot completions <shell>` prints completion scripts to stdout. Supported
shells are `bash`, `zsh`, `fish`, `powershell`, and `elvish`.

```sh
treeboot completions bash > ~/.local/share/bash-completion/completions/treeboot
treeboot completions zsh > ~/.zfunc/_treeboot
treeboot completions fish > ~/.config/fish/completions/treeboot.fish
treeboot completions powershell
treeboot completions elvish
```

The command does not install files or inspect the current repository while
printing the script. The generated script calls back into `treeboot` at
completion time, so source arguments for `copy`, `symlink`, and `sync` complete
from the resolved root checkout. Redirect the output to the path your shell or
package manager expects.

## Safety

`treeboot` is conservative by default:

- existing copy and symlink targets are skipped
- duplicate configured targets are config errors
- file targets must stay inside the current worktree
- `treeboot run` only belongs in repositories whose setup files you trust
- executable init scripts run before TOML config unless `--no-init-script` or
  `--config <path>` is provided
- `--strict` fails on existing copy/symlink targets and rejects sync operations
- `--force` is the explicit mode for replacing existing file-operation targets

The trust boundary includes `.treeboot.toml`, `treeboot.toml`,
`.config/treeboot/config.toml`, executable `.treeboot.sh`, `.treebootrc`,
`.config/treeboot/init`, and configured commands. Use `treeboot config` to
inspect TOML without execution.

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

After crates.io publication, Cargo users can install with:

```sh
cargo install treeboot
```

After installing the binary, generate shell completion scripts with
`treeboot completions <shell>` and install them according to your shell or
package manager conventions.

Releases are expected to include archives, raw executable assets,
`config.schema.json`, checksums, SBOMs, and provenance attestations. GPG
checksum signing and macOS
signing/notarization are planned distribution hardening work.

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
mise run clean
mise run coverage
mise run coverage:missing
mise run deps
mise run doctor
mise run format
mise run format:check
mise run generate
mise run generate:check
mise run generate:schema:check
mise run harness:check
mise run hooks:install
mise run lint
mise run msrv
mise run release:check
mise run test
mise run test:core
mise run test:cli
```

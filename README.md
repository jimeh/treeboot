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
new worktree often needs the same local setup every time: copy local env
overrides, link shared tooling, install dependencies, or run a project setup
command.

Instead of repeating those steps across configuration files for Codex, Claude
Code, Conductor, Superset, shell scripts, and team docs, put them in one place
and run:

```sh
treeboot
```

## Why

Git worktrees are cheap to create, but project-local state usually is not. Most
real projects have files and commands that are intentionally not committed:

- `.env.local`, `.env.development.local`, and `.env.test.local`
- `mise.local.toml`
- local agent or editor config
- shared scripts
- language runtime installs
- dependency installation commands

When creating a new worktree, those files typically need to be copied or
symlinked from the root checkout, and commands need to be run to install
dependencies or perform other setup.

`treeboot` gives those setup steps a single home.

## Quick Start

Add a `.treeboot.toml` to the repository root. For example:

```toml
#:schema https://github.com/jimeh/treeboot/releases/latest/download/config.schema.json

copy = [
  ".env.local",
  ".env.development.local",
  ".env.test.local",
  "mise.local.toml",
]

symlink = [
  "config/master.key",
]

commands = [
  "bundle install",
  "pnpm install",
]
```

After creating a new worktree, run:

```sh
treeboot
```

`treeboot` looks for a treeboot config file in the current worktree, discovers
the root checkout, and performs the configured copy, symlink, and command
operations.

By default, copy and symlink operations are idempotent. If a target already
exists, `treeboot` reports it and leaves it alone.

Missing copy, symlink, and sync sources are skipped by default. That makes it
safe to list several local-only files and let each worktree apply only the ones
that exist in the root checkout.

Commands always run, so they should be idempotent or otherwise safe and fast to
run repeatedly.

## Install

The recommended usage pattern is to make `treeboot` a project-local [mise][]
tool, usually scoped to a bootstrap task. For example in `mise.toml`:

[mise]: https://mise.jdx.dev/

```toml
[tasks.treeboot]
description = "Bootstrap the current worktree with treeboot"
tools."github:jimeh/treeboot" = "latest"
run = "treeboot"
```

Then contributors and agents can run:

```sh
mise run treeboot
```

For a global `mise` install:

```sh
mise use -g github:jimeh/treeboot
treeboot --version
```

Prebuilt binaries are available from
[GitHub Releases](https://github.com/jimeh/treeboot/releases), and Cargo users
can install from crates.io:

```sh
cargo install treeboot
```

Treeboot requires Git 2.36 or newer.

After installing the binary, generate shell completion scripts with
`treeboot completions <shell>` and install them according to your shell or
package manager conventions.

## Config

The default config file is:

```text
.treeboot.toml
```

The common top-level config keys are:

```toml
#:schema https://github.com/jimeh/treeboot/releases/latest/download/config.schema.json

strict = false
default_ignore = [".DS_Store", "Thumbs.db"]
dangerously_allow_sources_outside_root = false
dangerously_allow_targets_outside_worktree = false

copy = [
  ".env.local",
  ".env.development.local",
  ".env.test.local",
  "mise.local.toml",
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
`required = true` on a file object when a missing source should cause treeboot
to fail.

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

printf 'treeboot root directory: %s\n' "$root_path"
printf 'treeboot worktree directory: %s\n' "$(pwd)"
mise install
```

Use `treeboot run --no-init-script` to ignore executable init scripts and use
normal config discovery instead.

## Commands

`treeboot` and `treeboot run` are equivalent:

```sh
treeboot
treeboot run
treeboot status
treeboot config
treeboot config --format json
treeboot config --json
treeboot config --yaml
treeboot check
treeboot doctor
treeboot env
treeboot schema
treeboot schema --output config.schema.json
treeboot version
treeboot copy .env.local
treeboot symlink .tool-versions
treeboot sync shared/config --compare checksum
treeboot completions bash
```

`treeboot status` prints the detected worktree, root checkout, default branch,
config file, and init script discovery status without parsing config or running
scripts.

`status`, `config`, `check`, `doctor`, `env`, and `version` support
`--format text|json|yaml`, with `--json` and `--yaml` shortcuts for structured
output.

Useful options:

```sh
treeboot run --dry-run
treeboot run --strict
treeboot run --force
treeboot run --root /path/to/root-checkout
treeboot run --no-init-script
treeboot copy .env.local mise.local.toml --target local
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

Init scripts and configured commands receive these variables:

- `TREEBOOT_ROOT_PATH`: root checkout used as the source for file operations.
- `TREEBOOT_WORKTREE_PATH`: current worktree where setup is applied.
- `TREEBOOT_DEFAULT_BRANCH`: best-effort default branch name.

Use `treeboot env` to print the effective environment treeboot exposes to init
scripts and configured commands.

These environment variables can override config defaults:

- `TREEBOOT_STRICT`: enables strict validation and conflict handling.
- `TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT`: allows file operation
  sources outside `TREEBOOT_ROOT_PATH`.
- `TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE`: allows file operation
  targets outside `TREEBOOT_WORKTREE_PATH`.

## Schema

The JSON Schema for `.treeboot.toml` is published with each GitHub Release as
`config.schema.json`:

```text
https://github.com/jimeh/treeboot/releases/latest/download/config.schema.json
```

The same schema is checked into the repository at:

```text
schemas/treeboot.schema.json
```

Use `treeboot schema` to print the embedded schema from the installed binary, or
`treeboot schema --output <path>` to write it to a file.

## Project Status

`treeboot` is feature-complete for the core worktree bootstrap workflow in
[spec v1.16.0](./docs/SPEC.md). It supports:

- `run`, `status`, `config`, `check`, `doctor`, `env`, `schema`, `version`,
  `init`, `copy`, `symlink`, `sync`, and `completions`
- Git worktree, root checkout, and default-branch discovery
- declarative TOML config parsing, inspection, validation, and execution
- copy, symlink, and sync file operations with copy/sync path ignore rules
- command execution with treeboot environment variables
- executable init-script discovery and execution
- check/doctor/env introspection and embedded version/schema metadata
- JSON and YAML report output for inspection commands
- shell completion generation, including root-relative manual source completion
- release asset packaging and checked-in config schema generation

Remaining work is mostly release hardening, distribution polish, and follow-up
documentation. The spec is the compatibility contract; this README is the short
human-facing summary.

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
mise run format:markdown
mise run format:markdown:check
mise run format:rust
mise run format:rust:check
mise run generate
mise run generate:check
mise run generate:schema:check
mise run harness:check
mise run hooks:install
mise run lint
mise run lint:fix
mise run lint:markdown
mise run lint:rust
mise run msrv
mise run release:check
mise run test
mise run test:core
mise run test:cli
```

## License

[MIT](LICENSE)

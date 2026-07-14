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

New Git worktrees often need the same local setup: copy environment overrides,
link shared tooling, install dependencies, and run project setup commands.
`treeboot` puts those steps in one repo-local contract that works for people,
coding agents, editors, and orchestration tools.

Instead of maintaining separate setup instructions for every tool, add
`.treeboot.toml` to the repository and use one command:

```sh
treeboot
```

## Add treeboot to a project

The recommended setup uses [mise][] to make `treeboot` available across the
project and provide a standard bootstrap task.

[mise]: https://mise.jdx.dev/

### 1. Add the tool and task

Add these entries to the project's `mise.toml`, merging them into existing
`[tools]` and task sections when necessary:

```toml
[tools]
"github:jimeh/treeboot" = "latest"

[tasks.treeboot]
description = "Bootstrap the current worktree with treeboot"
run = "treeboot"
```

Keeping `treeboot` in the project-wide tool list makes it available to other
tasks and direct commands as well as the bootstrap task.

### 2. Describe the worktree setup

Add `.treeboot.toml` at the repository root:

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

### 3. Bootstrap each new worktree

Run this from the new worktree:

```sh
mise run treeboot
```

Missing copy, symlink, and sync sources are skipped by default, so the config
can list local files that only some contributors have. Existing copy and symlink
targets are also left alone by default.

Commands always run. Keep them idempotent, or delegate to a project setup task
that is safe to run repeatedly.

## How it works

`treeboot` runs from the current worktree and discovers the repository's root
checkout. The root checkout supplies local files that Git does not carry into a
new worktree; the current worktree receives those files and runs the configured
commands.

Typical inputs include:

- local environment files such as `.env.local`
- `mise.local.toml` and language runtime configuration
- local agent, editor, or shared-tool configuration
- dependency installation and project setup commands

File operations are planned and validated before any changes are made. Copy and
symlink operations are conservative by default, while `sync` actively reconciles
a target with its source.

## Configuration

The default config file is `.treeboot.toml`. Its main operations are:

| Key        | Behavior                                                         |
| ---------- | ---------------------------------------------------------------- |
| `copy`     | Copy a file or directory once, leaving an existing target alone. |
| `symlink`  | Create a relative link back to the root checkout.                |
| `sync`     | Reconcile a target with its source on every run.                 |
| `commands` | Run setup commands sequentially after file operations.           |

A more complete config can mix short string entries with objects:

```toml
#:schema https://github.com/jimeh/treeboot/releases/latest/download/config.schema.json

strict = false
default_ignore = [".DS_Store", "Thumbs.db"]

copy = [
  ".env.local",
  { source = ".env.test.local", required = true },
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
  { name = "Set up the project", run = "mise run setup" },
]
```

String file entries use the same source and target path. Object entries can use
a different target and set options such as `required = true`. Directory sync
preserves target-only files by default; set `delete = true` to remove them.

Commands run in declaration order. For parallel setup, put the parallel work
behind one task-runner command such as `mise run setup`.

The [JSON Schema](#schema) provides editor completion and documents all config
fields. The full observable behavior is defined by the
[treeboot specification](./docs/SPEC.md).

## Inspect and troubleshoot

Use the inspection commands before running an unfamiliar setup contract or when
diagnosing discovery and validation problems:

```sh
treeboot status        # Show the detected worktree, root, and config
treeboot config        # Print normalized TOML config without executing it
treeboot check         # Validate the setup plan without applying it
treeboot doctor        # Run discovery and configuration diagnostics
treeboot env           # Print treeboot-owned command environment variables
treeboot run --dry-run # Preview file operations and commands
```

`status`, `config`, `check`, `doctor`, `env`, and `version` support
`--format text|json|yaml`, with `--json` and `--yaml` shortcuts.

If no config is found, `treeboot` prints an info message and exits successfully.
Add `--strict` when that should be an error.

## Safety and trust

`treeboot` is conservative by default:

- existing copy and symlink targets are skipped
- missing file-operation sources are skipped unless marked as required
- duplicate configured targets are rejected
- file targets must stay inside the current worktree
- `--strict` rejects existing copy/symlink targets and sync operations
- `--force` explicitly allows replacement by file operations

Setup files can run arbitrary project commands. Only run `treeboot` in
repositories you trust. The trust boundary includes `.treeboot.toml`,
`treeboot.toml`, `.config/treeboot/config.toml`, and configured commands.

Use `treeboot config` to inspect declarative config without execution, or
`treeboot run --skip-commands` to apply only configured file operations.

## CLI reference

`treeboot` and `treeboot run` are equivalent.

| Purpose         | Commands                                     |
| --------------- | -------------------------------------------- |
| Bootstrap       | `run`                                        |
| Inspect         | `status`, `config`, `check`, `doctor`, `env` |
| File operations | `copy`, `symlink`, `sync`                    |
| Utilities       | `init`, `schema`, `version`, `completions`   |

Common examples:

```sh
treeboot run --dry-run
treeboot run --strict
treeboot run --force
treeboot run --root /path/to/root-checkout
treeboot copy .env.local mise.local.toml --target local
treeboot sync shared/config --delete --dry-run
treeboot init
```

`treeboot init` creates `.treeboot.toml` by default. `treeboot init --config` is
an explicit spelling of the same operation. Existing targets, including
symlinks, are never replaced.

## Installation alternatives

For a global mise install:

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

## Custom scripts

Declarative commands can execute any custom project script:

```toml
commands = [
  { run = "./scripts/bootstrap-worktree.sh" },
]
```

Configured commands run from the worktree root by default, inherit the
`TREEBOOT_*` environment, and run after file operations. They receive no
automatic positional `$1`; scripts should read `TREEBOOT_ROOT_PATH`, or the
config should pass it explicitly. `--skip-commands` omits configured commands,
and `--dry-run` reports them without execution.

Legacy `.treeboot.sh`, `.treebootrc`, and `.config/treeboot/init` files have no
special meaning and are treated as ordinary repository files. The former
`--no-init-script` and `init --script` options are no longer accepted.

## Environment

Configured commands receive:

- `TREEBOOT_ROOT_PATH`: root checkout used as the file-operation source.
- `TREEBOOT_WORKTREE_PATH`: current worktree where setup is applied.
- `TREEBOOT_DEFAULT_BRANCH`: best-effort default branch name.

Configuration defaults can be overridden with `TREEBOOT_STRICT`,
`TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT`, and
`TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE`.

Use `treeboot env` to print the effective treeboot-owned environment.

## Schema

The JSON Schema for `.treeboot.toml` is published with every GitHub Release:

```text
https://github.com/jimeh/treeboot/releases/latest/download/config.schema.json
```

It is also checked into this repository at
[`schemas/treeboot.schema.json`](./schemas/treeboot.schema.json). Use
`treeboot schema` to print the embedded schema or
`treeboot schema --output <path>` to write it to a file.

## Shell completions

`treeboot completions <shell>` prints a completion script for `bash`, `zsh`,
`fish`, `powershell`, or `elvish`:

```sh
treeboot completions bash > ~/.local/share/bash-completion/completions/treeboot
treeboot completions zsh > ~/.zfunc/_treeboot
treeboot completions fish > ~/.config/fish/completions/treeboot.fish
```

The command only prints the script; it does not install completion files.

## Project status

`treeboot` is feature-complete for its core worktree bootstrap workflow. The
current compatibility contract is [spec v2.0.0](./docs/SPEC.md); this README is
the shorter, human-facing guide.

The name `treeboot` means "worktree bootstrap."

## Development

This project uses mise for tools and tasks:

```sh
mise run setup  # Set up the development environment
mise run check  # Normal pre-handoff validation
mise run verify # Broad local verification
mise run ci     # Run the CI task set
```

See [`mise.toml`](./mise.toml) for targeted tasks and [`AGENTS.md`](./AGENTS.md)
for contributor and coding-agent guidance.

## License

[MIT](LICENSE)

<div align="center">

<img width="196px" src="./img/treeboot.svg" alt="Logo">

# treeboot

**Set up Git worktrees from one file and clean up their resources before
removal.**

[![GitHub Release](https://img.shields.io/github/v/release/jimeh/treeboot?logo=github&label=Release)](https://github.com/jimeh/treeboot/releases/latest)
[![crates.io](https://img.shields.io/crates/v/treeboot?logo=rust&label=crates.io)](https://crates.io/crates/treeboot)
[![docs.rs](https://img.shields.io/docsrs/treeboot-core?logo=docs.rs&label=docs.rs)](https://docs.rs/treeboot-core)
[![GitHub Issues](https://img.shields.io/github/issues/jimeh/treeboot?logo=github&label=Issues)](https://github.com/jimeh/treeboot/issues)
[![GitHub Pull Requests](https://img.shields.io/github/issues-pr/jimeh/treeboot?logo=github&label=PRs)](https://github.com/jimeh/treeboot/pulls)
[![License](https://img.shields.io/github/license/jimeh/treeboot?label=License)](https://github.com/jimeh/treeboot/blob/main/LICENSE)

</div>

New Git worktrees often need the same local setup: copy environment overrides,
link shared tooling, install dependencies, and run project setup commands.
`treeboot` stores those steps in `.treeboot.toml`, so contributors, coding
agents, editors, and orchestration tools can run the same setup.

Projects can also declare teardown commands for resources that belong to one
worktree, such as databases, containers, or preview environments. Run those
commands explicitly before another tool removes the worktree.

Instead of maintaining separate setup instructions for every tool, add one
config file to the repository and run `treeboot` in each new worktree:

```sh
treeboot
```

## Add treeboot to a project

Treeboot requires Git 2.36 or newer. The recommended setup also uses [mise][] to
make `treeboot` available across the project and provide a standard bootstrap
task. If the project does not use mise, see
[Installation alternatives](#installation-alternatives).

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

### 2. Create and edit the config

From the repository root, create a starter config:

```sh
mise exec -- treeboot init
```

This runs `treeboot init` with the project-managed Treeboot version. The command
creates `.treeboot.toml` and never replaces an existing file or symlink. The
generated config contains no setup or teardown commands:

```toml
#:schema https://github.com/jimeh/treeboot/releases/latest/download/config.schema.json

copy = [
  ".env.local",
]

symlink = [
]

commands = [
]

teardown_commands = [
]
```

Missing file-operation sources are skipped by default, so the starter config
does nothing when `.env.local` does not exist. Edit the file to match the paths
and commands used by the project.

<details>
<summary>See a Rails and Node.js project example</summary>

This example copies local environment files, links a shared Rails key, installs
dependencies, and defines teardown for worktree-specific services. Replace the
paths and commands with ones that belong to the project.

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

teardown_commands = [
  { name = "Stop services", run = "docker compose down" },
  { name = "Drop database", run = "mise run db:drop" },
]
```

</details>

Setup commands run on every bootstrap. Keep them idempotent, or delegate to a
project setup task that is safe to run repeatedly. Teardown commands run only
through `treeboot teardown`.

### 3. Preview and bootstrap a linked worktree

A Treeboot config can run arbitrary project commands. Only run it in
repositories you trust. Commit `mise.toml` and `.treeboot.toml`, then create or
enter a linked worktree. From that worktree, inspect the normalized config and
preview the planned file operations and commands:

```sh
mise exec -- treeboot config
mise exec -- treeboot run --dry-run
```

Then bootstrap the current worktree:

```sh
mise run treeboot
```

Run the same task in each new worktree. Existing copy and symlink targets are
left alone by default.

### Optional: tell coding agents when to bootstrap

If the project already has an `AGENTS.md`, `CLAUDE.md`, or equivalent
instruction file, add a rule like this. If one instruction file references
another, add the rule to the referenced file instead of duplicating it.

```markdown
## Worktree bootstrap

If a linked worktree is missing files or dependencies needed for development,
run `mise run treeboot` before setting it up manually. The task follows
`.treeboot.toml`, including its configured file operations and setup commands.
```

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

| Key                 | Behavior                                                         |
| ------------------- | ---------------------------------------------------------------- |
| `copy`              | Copy a file or directory once, leaving an existing target alone. |
| `symlink`           | Create a relative link back to the root checkout.                |
| `sync`              | Reconcile a target with its source on every run.                 |
| `worktree_id`       | Tune the compact path-derived ID passed to lifecycle commands.   |
| `worktree_slug`     | Tune the readable slug passed to lifecycle commands.             |
| `commands`          | Run setup commands sequentially after file operations.           |
| `teardown_commands` | Run explicitly approved commands before external removal.        |

File operations in the starter and Rails and Node.js examples above use short
string entries. Object entries can use a different target and set options such
as `required = true`. Directory sync preserves target-only files by default; set
`delete = true` to remove them.

Commands run in declaration order. For parallel setup, put the parallel work
behind one task-runner command such as `mise run setup`.

Teardown commands use the same shell/direct command fields, `cwd`, `env`, and
`allow_failure` behavior. They run only through `treeboot teardown`; bootstrap
never runs them.

The [JSON Schema](#schema) provides editor completion and documents all config
fields. The full observable behavior is defined by the
[treeboot specification](./crates/treeboot-spec/SPEC.md).

## Inspect and troubleshoot

Use the inspection commands before running an unfamiliar setup contract or when
diagnosing discovery and validation problems:

```sh
treeboot status        # Show the detected worktree, root, and config
treeboot config        # Print normalized TOML config without executing it
treeboot check         # Validate bootstrap and teardown plans
treeboot doctor        # Run discovery and configuration diagnostics
treeboot env           # Print effective treeboot-owned command environment
treeboot worktree id [PATH]   # Print a compact path-derived ID
treeboot worktree slug [PATH] # Print the matching readable slug
treeboot worktree list        # List registered IDs, slugs, and paths
treeboot run --dry-run # Preview file operations and commands
treeboot teardown --dry-run # Preview teardown commands without prompting
```

`status`, `config`, `check`, `doctor`, `env`, `version`, and all `worktree` leaf
commands support `--format text|json|yaml`, with `--json` and `--yaml`
shortcuts.

Use `treeboot worktree path <ID>` to resolve an exact ID in the current
repository. Repository-wide lookup and listing use each worktree's own config,
skip stale registered paths, and report ID collisions instead of choosing a
path. The [treeboot specification](./crates/treeboot-spec/SPEC.md) defines
platform-specific path and structured-output behavior.

If no config is found, `treeboot` prints an info message and exits successfully.
Add `--strict` to bootstrap when that should be an error. Missing discovered
config is always a teardown no-op; an explicit `--config` must exist.

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
`treeboot.toml`, `.config/treeboot/config.toml`, and configured bootstrap and
teardown commands. Teardown commands may delete resources outside the worktree.

Use `treeboot config` to inspect declarative config without execution, or
`treeboot run --skip-commands` to apply only configured file operations. Use
`treeboot teardown --dry-run` to inspect teardown commands.

`treeboot teardown` never removes a Git worktree or branch. It requires terminal
confirmation or the long-only `--yes` flag before execution. A refusal or
non-interactive run without `--yes` exits unsuccessfully, so it can safely guard
an external removal:

```sh
treeboot teardown --worktree "$path" --yes &&
  git worktree remove "$path"
```

## CLI reference

`treeboot` and `treeboot run` are equivalent.

| Purpose         | Commands                                                 |
| --------------- | -------------------------------------------------------- |
| Bootstrap       | `run`                                                    |
| Teardown        | `teardown`                                               |
| Inspect         | `status`, `config`, `check`, `doctor`, `env`, `worktree` |
| File operations | `copy`, `symlink`, `sync`                                |
| Utilities       | `init`, `schema`, `version`, `completions`               |

Common examples:

```sh
treeboot run --dry-run
treeboot run --strict
treeboot run --force
treeboot run --root /path/to/root-checkout
treeboot teardown --dry-run
treeboot teardown --worktree ../feature --yes
treeboot copy .env.local mise.local.toml --target local
treeboot sync shared/config --delete --dry-run
```

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

## Custom scripts

Declarative bootstrap and teardown commands can execute any custom project
script:

```toml
commands = [
  { run = "./scripts/bootstrap-worktree.sh" },
]

teardown_commands = [
  { run = "./scripts/teardown-worktree.sh" },
]
```

Configured commands run from the worktree root by default, inherit the
`TREEBOOT_*` environment, and receive no automatic positional `$1`; scripts
should read `TREEBOOT_ROOT_PATH`, `TREEBOOT_WORKTREE_ID`, and
`TREEBOOT_WORKTREE_SLUG`, or the config should pass values explicitly. Bootstrap
commands run after file operations; `--skip-commands` omits them. Teardown
commands run only through `treeboot teardown` after approval. Both commands
support `--dry-run` reporting without execution.

## Environment

Configured commands receive:

- `TREEBOOT_ROOT_PATH`: root checkout used as the file-operation source.
- `TREEBOOT_WORKTREE_PATH`: current worktree where setup is applied.
- `TREEBOOT_WORKTREE_ID`: compact path-derived ID, such as `k7m2qx`.
- `TREEBOOT_WORKTREE_SLUG`: readable path-derived slug, such as
  `feature-login-k7m2qx`, for per-worktree resources.
- `TREEBOOT_DEFAULT_BRANCH`: best-effort default branch name.

Configuration defaults can be overridden with `TREEBOOT_STRICT`,
`TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT`, and
`TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE`. These affect bootstrap
file planning, not command-only teardown.

Use `treeboot env` to print the effective treeboot-owned environment.
`treeboot worktree id` and `treeboot worktree slug` print the current values or
derive them for a supplied path. `treeboot worktree path <ID>` resolves an ID in
the current repository, and `treeboot worktree list` inventories its worktrees.

The ID defaults to a six-character lowercase Crockford base32 digest prefix. The
slug defaults to at most 48 DNS-label-compatible characters and ends with the
complete ID. Configure them through the top-level `worktree_id` and
`worktree_slug` objects. The
[treeboot specification](./crates/treeboot-spec/SPEC.md) defines path
normalization, platform restrictions, and config-aware lookup behavior.

## Schema

The JSON Schema for `.treeboot.toml` is published with every GitHub Release:

```text
https://github.com/jimeh/treeboot/releases/latest/download/config.schema.json
```

It is also checked into this repository at
[`crates/treeboot-spec/assets/treeboot.schema.json`](./crates/treeboot-spec/assets/treeboot.schema.json).
Use `treeboot schema` to print the embedded schema or
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

## Compatibility suite

The publishable [`treeboot-spec`](./crates/treeboot-spec/README.md) crate owns
the canonical specification, schema, and black-box CLI conformance suite. Its
CLI can test the official binary or an independent implementation:

```sh
treeboot-spec test -- /path/to/treeboot
treeboot-spec test --format json -- /path/to/treeboot
```

The suite tests only the language-agnostic CLI contract. It does not specify the
`treeboot-core` Rust API.

## Project status

Treeboot bootstraps linked worktrees and runs explicitly approved teardown
commands. The current language-agnostic CLI compatibility contract is
[spec v2.5.1](./crates/treeboot-spec/SPEC.md); this README is the shorter,
human-facing guide.

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

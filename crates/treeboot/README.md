<div align="center">

<img width="160px" src="https://github.com/jimeh/treeboot/raw/HEAD/img/treeboot.svg?sanitize=true" alt="treeboot logo">

# treeboot

Bootstrap new Git worktrees from one repo-local setup file.

[![crates.io](https://img.shields.io/crates/v/treeboot?logo=rust&label=crates.io)](https://crates.io/crates/treeboot)
[![License](https://img.shields.io/github/license/jimeh/treeboot?label=License)](https://github.com/jimeh/treeboot/blob/main/LICENSE)

</div>

`treeboot` is a CLI for teams and agents that create lots of Git worktrees. A
new worktree often needs the same local setup every time: copy local env
overrides, link shared tooling, install dependencies, or run a project setup
command.

Instead of repeating those steps across configuration files for Codex, Claude
Code, Conductor, Superset, shell scripts, and team docs, put them in
`.treeboot.toml` and run:

```sh
treeboot
```

## Install

The recommended usage pattern is to make `treeboot` a project-local [mise][]
tool, usually scoped to a bootstrap task:

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

## Example

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

Missing copy, symlink, and sync sources are skipped by default, so one config
can safely list several local-only files.

Commands always run, so they should be idempotent or otherwise safe and fast to
run repeatedly.

`treeboot` and `treeboot run` are equivalent. The CLI also includes
`status`, `config`, `check`, `doctor`, `env`, `schema`, `version`, `init`,
`copy`, `symlink`, `sync`, and `completions` subcommands.

See the [repository](https://github.com/jimeh/treeboot) for project details.

## License

MIT

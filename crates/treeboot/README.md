<div align="center">

<img width="160px" src="https://github.com/jimeh/treeboot/raw/HEAD/img/treeboot.svg?sanitize=true" alt="treeboot logo">

# treeboot

Bootstrap new Git worktrees from one repo-local setup file.

[![crates.io](https://img.shields.io/crates/v/treeboot?logo=rust&label=crates.io)](https://crates.io/crates/treeboot)
[![License](https://img.shields.io/github/license/jimeh/treeboot?label=License)](https://github.com/jimeh/treeboot/blob/main/LICENSE)

</div>

`treeboot` is a CLI for teams and agents that create lots of Git worktrees. A
new worktree often needs the same local setup every time: copy an `.env`, link
shared tooling, install dependencies, or run a project setup command. Put those
steps in `.treeboot.toml` and run:

```sh
treeboot
```

## Install

The primary binary distribution is GitHub Releases. The crate is also prepared
for crates.io publishing:

```sh
cargo install treeboot
```

## Example

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

Run from a linked worktree:

```sh
treeboot
```

`treeboot` and `treeboot run` are equivalent. The CLI also includes
`config`, `init`, `copy`, `symlink`, `sync`, and `completions` subcommands.

See the [repository](https://github.com/jimeh/treeboot) for project details.

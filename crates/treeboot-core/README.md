# treeboot-core

Reusable Rust library for bootstrapping Git worktrees from a repo-local
`treeboot` setup contract.

This crate contains the public API used by the `treeboot` CLI. It handles Git
worktree discovery, init script discovery, declarative config parsing,
validation, action planning, file operation execution, command execution, and
structured output events.

## API Shape

Use command-shaped facade functions when you want the same behavior as the CLI:

```rust
use treeboot_core::{RunOptions, Reporter, run};

fn bootstrap(reporter: &mut dyn Reporter) -> treeboot_core::Result<()> {
    let report = run(RunOptions::default(), reporter)?;
    let _ = report;

    Ok(())
}
```

Use lower-level types when embedding pieces of the workflow:

```rust
use treeboot_core::{ActionPlan, ActionPlanOptions, Config, Worktree};

fn plan_bootstrap(
    context: &Worktree,
    config: &Config,
) -> treeboot_core::Result<ActionPlan> {
    ActionPlan::from_manifest(config, context, ActionPlanOptions::default())
}
```

The crate exposes typed errors through `treeboot_core::Error` and avoids
CLI-specific dependencies.

## Relationship to `treeboot`

`treeboot-core` is the reusable library crate. The `treeboot` package provides
the command-line interface and user-facing reporting on top of this API.

See the [repository](https://github.com/jimeh/treeboot) for project details.

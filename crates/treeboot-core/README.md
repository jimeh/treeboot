<div align="center">

<img width="160px" src="https://github.com/jimeh/treeboot/raw/HEAD/img/treeboot.svg?sanitize=true" alt="treeboot logo">

# treeboot-core

Reusable Rust library for bootstrapping Git worktrees from a repo-local
`treeboot` setup contract.

[![crates.io](https://img.shields.io/crates/v/treeboot-core?logo=rust&label=crates.io)](https://crates.io/crates/treeboot-core)
[![docs.rs](https://img.shields.io/docsrs/treeboot-core?logo=docs.rs&label=docs.rs)](https://docs.rs/treeboot-core)
[![License](https://img.shields.io/github/license/jimeh/treeboot?label=License)](https://github.com/jimeh/treeboot/blob/main/LICENSE)

</div>

This crate contains the public API used by the `treeboot` CLI. It handles Git
worktree discovery, declarative config parsing, validation, action planning,
file operation execution, command execution, and structured output events.

This crate exposes the same workflow as typed Rust APIs for callers that want to
embed treeboot behavior directly.

## API Shape

Use command-shaped facade functions when you want the same behavior as the CLI:

```rust
use treeboot_core::{EnvironmentInput, RunOptions, Reporter, run};

fn bootstrap(reporter: &mut dyn Reporter) -> treeboot_core::Result<()> {
    let report = run(
        RunOptions {
            environment: EnvironmentInput::from_process_env(),
            ..RunOptions::default()
        },
        reporter,
    )?;
    let _ = report;

    Ok(())
}
```

`RunOptions::default()` and the other command-shaped option defaults are
environment-pure. Pass `EnvironmentInput::from_process_env()` when embedding the
CLI's process-environment compatibility behavior.

Use lower-level types when embedding pieces of the workflow. Action plans are
validated values: build them through constructors, then inspect them through
accessor methods before execution if needed.

```rust
use std::path::Path;

use treeboot_core::{ActionPlan, ActionPlanOptions, Config, Worktree};

fn plan_bootstrap(
    config_path: &Path,
    context: &Worktree,
    config: &Config,
) -> treeboot_core::Result<ActionPlan> {
    let plan = ActionPlan::from_manifest(
        config_path,
        config,
        context,
        ActionPlanOptions::default(),
    )?;
    let _file_count = plan.files().len();

    Ok(plan)
}
```

The crate exposes typed errors through `treeboot_core::Error` and avoids
CLI-specific dependencies.

## Relationship to `treeboot`

`treeboot-core` is the reusable library crate. The `treeboot` package provides
the command-line interface and user-facing reporting on top of this API.

See the [repository](https://github.com/jimeh/treeboot) for project details.

## License

MIT

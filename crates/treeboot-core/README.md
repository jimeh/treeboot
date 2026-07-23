<div align="center">

<img width="160px" src="https://github.com/jimeh/treeboot/raw/HEAD/img/treeboot.svg?sanitize=true" alt="treeboot logo">

# treeboot-core

Reusable Rust library for bootstrapping Git worktrees and running pre-removal
teardown from a repo-local `treeboot` setup contract.

[![crates.io](https://img.shields.io/crates/v/treeboot-core?logo=rust&label=crates.io)](https://crates.io/crates/treeboot-core)
[![docs.rs](https://img.shields.io/docsrs/treeboot-core?logo=docs.rs&label=docs.rs)](https://docs.rs/treeboot-core)
[![License](https://img.shields.io/github/license/jimeh/treeboot?label=License)](https://github.com/jimeh/treeboot/blob/main/LICENSE)

</div>

This crate contains the public API used by the `treeboot` CLI. It handles Git
worktree discovery, declarative config parsing, validation, action planning,
file operation execution, shared bootstrap/teardown command execution, and
structured output events.

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

Teardown is deliberately split into preparation and execution so an embedding
can approve the exact validated command plan before it runs. Preparation
distinguishes missing config, an empty teardown command collection, and a ready
`TeardownPlan`. The CLI owns terminal detection and prompting; core never reads
stdin. `execute_teardown` runs only the already prepared teardown plan and never
removes a worktree.

```rust
use treeboot_core::{
    Reporter, TeardownExecuteOptions, TeardownOptions, execute_teardown,
    prepare_teardown,
};

fn teardown(
    approved: bool,
    reporter: &mut dyn Reporter,
) -> treeboot_core::Result<bool> {
    let prepared = prepare_teardown(TeardownOptions::default(), reporter)?;
    let Some(plan) = prepared.plan() else {
        return Ok(true);
    };

    if !approved {
        return Ok(false);
    }

    execute_teardown(
        plan,
        TeardownExecuteOptions::default(),
        reporter,
    )?;
    Ok(true)
}
```

When this returns `Ok(false)`, embedding callers should report an unsuccessful
status (for example, exit 1) and must not remove the worktree.

Bootstrap `ActionPlan` and `TeardownPlan` keep separate phase contents while
sharing command planning, cwd/environment validation, and command runtime
semantics.

## Public Config Construction

The normalized config graph and resolved `Worktree` context are
`#[non_exhaustive]`. Downstream code can read their public fields, but must use
`..` when destructuring and cannot construct them with struct literals. This
lets future treeboot releases add fields without breaking exhaustive downstream
patterns.

Use the stable construction paths instead:

- `Config::default()` for an empty normalized config
- `Worktree::from_parts(...)` for a synthetic resolved context
- `SourceSpan::new(...)` for source attribution
- `CommandOperation::shell(...)` or `CommandOperation::direct(...)`
- operation-specific `FileOperation` constructors, or
  `FileOperation::from_manual_options(...)`

For example, replace a source span literal:

```rust,ignore
let span = SourceSpan {
    start: 0,
    end: 0,
    line: 1,
    column: 1,
};
```

with:

```rust,ignore
let span = SourceSpan::new(0, 0, 1, 1);
```

Migrate exhaustive config destructuring from:

```rust,ignore
let Config {
    options,
    files,
    commands,
} = config;
```

to:

```rust,ignore
let Config {
    options,
    files,
    commands,
    teardown_commands,
    ..
} = config;
```

Direct field reads remain supported. Removing a field, changing its type, or
changing one of the intentionally closed config enums can still be a breaking
change.

The crate exposes typed errors through `treeboot_core::Error` and avoids
CLI-specific dependencies.

## Relationship to `treeboot`

`treeboot-core` is the reusable library crate. The `treeboot` package provides
the command-line interface and user-facing reporting on top of this API.

See the [repository](https://github.com/jimeh/treeboot) for project details.

## License

MIT

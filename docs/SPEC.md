# treeboot Specification v2.1.0

A portable worktree lifecycle helper that lets every coding agent, editor, and
orchestration tool run the same repo-local bootstrap and teardown commands.

**Tags:** compatibility contract, Rust executable, TOML config, idempotent
default, configured commands, teardown commands, agent-tool aliases

| Term            | Description                                                |
| --------------- | ---------------------------------------------------------- |
| Default command | `treeboot` == `treeboot run`                               |
| Root path       | Source checkout for copy, symlink, and sync operations     |
| Worktree path   | Current worktree root where operations execute             |
| Conflict policy | Skip existing by default, strict validates, force replaces |
| Primary file    | `.treeboot.toml`                                           |
| Teardown        | Explicit commands run before an external worktree removal  |

treeboot requires Git 2.36 or newer.

## Intent: One setup contract for many tools

Agentic coding tools already create isolated Git worktrees. The repeated pain is
everything Git intentionally leaves behind: ignored files, local credentials,
dependency caches, and per-worktree setup commands.

### Goals

- Provide one repo-local worktree bootstrap command.
- Avoid duplicate setup files across agent tools.
- Make common copy, symlink, and sync operations declarative.
- Allow custom setup through configured commands.
- Allow explicit cleanup before another tool removes a linked worktree.
- Be safe and idempotent by default.
- Ship as a small, portable executable.

### Non-goals

- Create Git worktrees.
- Remove Git worktrees or branches.
- Install or enforce hooks around `git worktree remove`.
- Manage long-running dev servers.
- Allocate per-worktree ports.
- Replace tool-specific setup systems entirely.
- Manage secrets beyond user-configured file operations.

### Design rule

treeboot should be boring to run repeatedly. File operations are idempotent by
default. Bootstrap and teardown commands always run when their phase is
executed, so users must make configured commands safe to rerun when that
matters.

### Implementation bar

The first implementation should target the full documented behavior in this
spec.

## CLI surface: Fourteen subcommands, one default path

The common integration point is intentionally short: `treeboot`. Tool-specific
setup hooks only need to invoke the declarative treeboot config.

### `treeboot run`

Runs worktree bootstrap. This is also the implicit command when no subcommand is
provided.

```sh
treeboot
treeboot run
treeboot run --dry-run
treeboot run --verbose
treeboot run --strict
treeboot run --force
treeboot run --root /path/to/root-checkout
treeboot run --config .treeboot.toml
treeboot run --skip-commands
```

### `treeboot teardown`

Runs configured teardown commands for a linked worktree without removing the
worktree itself.

```sh
treeboot teardown
treeboot teardown --dry-run
treeboot teardown --yes
treeboot teardown --worktree ../feature-branch
treeboot teardown --worktree ../feature-branch --yes
treeboot teardown --root /path/to/root-checkout
treeboot teardown --config .treeboot.toml
```

The target defaults to the process working directory. `--worktree <path>` may
point anywhere inside another linked worktree and resolves to that worktree's
Git top level. Teardown rejects Git's main/root checkout before prompting.

`--yes` is long-only and grants non-interactive approval. Without `--yes`,
teardown prompts only when stdin is a terminal. `--dry-run` validates and
reports every teardown command without prompting, requiring `--yes`, or spawning
a process.

Teardown is command-only. It never applies configured file operations, runs
bootstrap `commands`, removes a worktree, or deletes a branch.

Teardown does not accept `--force` or `--strict`. `--force` remains the
file-replacement option, and teardown has no file conflict policy. Use
`--config` when teardown must require a specific config.

### `treeboot status`

Prints the detected worktree, root checkout, default branch, and config file
without parsing config or running configured commands. `treeboot info` is an
alias.

```sh
treeboot status
treeboot status --root /path/to/root-checkout
treeboot status --config .treeboot.toml
treeboot status --format json
treeboot status --format yaml
treeboot status --json
treeboot status --yaml
treeboot info
```

JSON and YAML output are defined in
[Structured output formats](#structured-output-formats).

### `treeboot version`

Prints version metadata and exits without discovering Git context or config.
`treeboot --version` and `treeboot -V` are global version flags that print
package and spec version details through the CLI parser's built-in version
handling.

```sh
treeboot version
treeboot version --format json
treeboot version --format yaml
treeboot version --json
treeboot version --yaml
treeboot --version
treeboot -V
```

Human-readable output is a compact, flag-like summary:

```text
treeboot 0.10.0 (spec 2.1.0)
```

JSON and YAML output are defined in
[Structured output formats](#structured-output-formats).

### `treeboot config`

Parses the selected TOML config and prints the normalized file, bootstrap
command, and teardown command operations without executing them.

```sh
treeboot config
treeboot config --config .treeboot.toml
treeboot config --format json
treeboot config --format yaml
treeboot config --json
treeboot config --yaml
```

This command is view-only. It is intended for validating and inspecting config
parsing behavior; editing config values is out of scope.

Human-readable text output lists normalized source and target values plus
behavior-affecting normalized fields such as `required`, `compare`, `delete`,
`symlinks`, `include`, `ignore`, `allow_failure`, `cwd`, and command `env`
values when present. It labels bootstrap and teardown command collections
separately and prints `(none)` for an empty collection. JSON and YAML output
emit the full normalized config structure.

### `treeboot check`

Validates the complete selected config contract without applying file operations
or running bootstrap or teardown commands.

```sh
treeboot check
treeboot check --root /path/to/root-checkout
treeboot check --config .treeboot.toml
treeboot check --strict
treeboot check --format json
treeboot check --format yaml
treeboot check --json
treeboot check --yaml
```

`check` resolves the same worktree context and config selection rules as `run`,
parses and normalizes declarative config, resolves bootstrap runtime policy, and
independently validates the bootstrap and teardown plans.

When declarative config is selected, config parse and normalization errors are
fatal to the whole contract. After normalization, bootstrap and teardown
semantic validation are independent. `check` evaluates both, preserves both
failures when both phases are invalid, reports them in bootstrap-then-teardown
order, and exits non-zero when either phase fails.

On success, human-readable output prints:

```text
treeboot: check ok
```

Run validation can produce non-fatal warnings, such as an include list that
matches no source paths. `check` prints each warning as a
`treeboot: warning: ...` line before the final `treeboot: check ok` line and
still exits zero.

JSON and YAML output are defined in
[Structured output formats](#structured-output-formats). Fatal errors still use
treeboot's normal error reporting and non-zero exit behavior.

### `treeboot init`

Creates a starter config.

```sh
treeboot init
treeboot init --config
treeboot init --path .treeboot.toml
```

The default path is `.treeboot.toml`. Existing init targets, including symlinks,
are never replaced.

### `treeboot schema`

Prints the bundled JSON Schema for treeboot config to stdout and exits without
discovering Git context or config.

```sh
treeboot schema
treeboot schema --output config.schema.json
treeboot schema -o config.schema.json
treeboot schema > config.schema.json
```

The emitted schema is the same config schema published as the release asset
`config.schema.json`. When `--output` is provided, treeboot writes the schema to
that path instead of stdout. Parent directories must already exist. Existing
regular files are replaced.

`schema` does not support `--format`, `--json`, or `--yaml`; the schema payload
is already JSON.

### `treeboot copy`

Runs one or more copy operations without running declarative config file
operations or configured commands.

```sh
treeboot copy .env.local
treeboot copy .env.local mise.local.toml --target local
treeboot copy templates/editorconfig --target .editorconfig
treeboot copy shared/config --symlinks preserve
treeboot copy shared/config --verbose
```

### `treeboot symlink`

Runs one or more symlink operations from the root path into the current
worktree.

```sh
treeboot symlink .tool-versions
treeboot symlink bin scripts --target .local
treeboot symlink shared/bin --target bin
```

### `treeboot sync`

Runs one or more sync operations without running declarative config file
operations or configured commands.

```sh
treeboot sync shared/config
treeboot sync shared/config shared/editor --target .config
treeboot sync shared/config --delete
treeboot sync shared/config --compare checksum
treeboot sync shared/config --verbose
```

### `treeboot completions`

Prints shell completion scripts for supported shells so package managers and
users can install them.

```sh
treeboot completions bash
treeboot completions zsh
treeboot completions fish
treeboot completions powershell
treeboot completions elvish
```

### `treeboot doctor`

Prints diagnostics for the current treeboot environment without applying file
operations or running configured commands.

```sh
treeboot doctor
treeboot doctor --root /path/to/root-checkout
treeboot doctor --config .treeboot.toml
treeboot doctor --strict
treeboot doctor --format json
treeboot doctor --format yaml
treeboot doctor --json
treeboot doctor --yaml
```

`doctor` checks Git availability, worktree discovery, root path discovery,
default branch discovery, child environment construction, config discovery, and
both bootstrap and teardown validation when config is selected. It reports
separate phase diagnostics from the same complete-config validation result used
by `check`. It is intended for human troubleshooting. Warnings do not fail the
command, but fatal discovery or config errors exit non-zero.

`--strict` makes doctor report missing config, root-checkout execution context,
and strict file-operation validation failures as fatal diagnostics while still
printing the diagnostics report when possible. `TREEBOOT_STRICT=true` applies
the same strict pre-config diagnostics and config validation. Config-level
`strict = true` applies to config validation only, because config-level options
are not loaded before root-checkout or missing-config checks.

JSON and YAML output are defined in
[Structured output formats](#structured-output-formats).

### `treeboot env`

Prints the environment variables treeboot passes to configured commands.

```sh
treeboot env
treeboot env --root /path/to/root-checkout
treeboot env --format json
treeboot env --format yaml
treeboot env --json
treeboot env --yaml
```

The text format is one `KEY=value` pair per line, sorted by variable name.
Values are resolved for the current worktree context. `env` does not parse
config, apply file operations, or run commands.

JSON and YAML output are defined in
[Structured output formats](#structured-output-formats).

`env` prints only the treeboot-owned child environment variables described in
[Environment variables](#compatibility-environment-variables). It does not print
the full process environment, per-command `env` overlays, or the config option
override variables that treeboot reads from its parent environment.

### Manual file operation commands

`copy`, `symlink`, and `sync` expose the same file operation engine used by
declarative config. Each command requires at least one source argument. Multiple
source arguments create multiple independent file operations. These commands
still discover the root path and worktree path, but they do not run configured
commands.

After worktree/root context checks, manual file operation commands discover and
parse config when one is present only to load top-level runtime policy. They
ignore the config's file operations, bootstrap commands, and teardown commands.
If config parsing is reached and the config is invalid, the manual command fails
before applying file operations.

With one source, `--target` is the exact target path. With multiple sources,
`--target` is a directory or path prefix joined with each source value. For
example, `treeboot copy .env.local mise.local.toml --target local` copies
`.env.local` to `local/.env.local` and `mise.local.toml` to
`local/mise.local.toml`.

Manual commands apply the same output contract as declarative file operations.
Multiple source arguments create multiple top-level file operations for output
and progress as well as execution. A command such as
`treeboot copy a b --target local` reports separate decisions for `a -> local/a`
and `b -> local/b`; it does not collapse them into one command-wide summary.

Operation-specific flags are valid only on the commands listed in the option
table. For example, using `--compare` on `copy` or `--symlinks` on `symlink` is
a CLI usage error and exits with code `2`.

| Option                                                     | Scope                                                         | Behavior                                                                                                                                                                                                                                                                    |
| ---------------------------------------------------------- | ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-r`, `--root <path>`                                      | run/teardown/status/config/check/copy/symlink/sync/doctor/env | Overrides the root checkout used for discovery and file-operation context.                                                                                                                                                                                                  |
| `-c`, `--config <path>`                                    | run/teardown/status/config/check/doctor                       | Uses one specific config file and skips config discovery. For teardown, relative paths resolve from the selected worktree.                                                                                                                                                  |
| `--worktree <path>`                                        | teardown                                                      | Selects the linked worktree to tear down. Defaults to the process working directory; a path inside a worktree resolves to its Git top level.                                                                                                                                |
| `-o`, `--format <text\|json\|yaml>`                        | status/version/config/check/doctor/env                        | Selects human-readable, JSON, or YAML output. Defaults to `text`.                                                                                                                                                                                                           |
| `-J`, `--json`                                             | status/version/config/check/doctor/env                        | Shortcut for `--format json`. Conflicts with `--format` and `--yaml`.                                                                                                                                                                                                       |
| `-Y`, `--yaml`                                             | status/version/config/check/doctor/env                        | Shortcut for `--format yaml`. Conflicts with `--format` and `--json`.                                                                                                                                                                                                       |
| `-V`, `--version`                                          | global                                                        | Prints package and spec version details and exits before command validation.                                                                                                                                                                                                |
| `-o`, `--output <path>`                                    | schema                                                        | Writes the bundled config schema to a file instead of stdout.                                                                                                                                                                                                               |
| `-S`, `--strict`                                           | run/check/copy/symlink/sync/doctor                            | Fails if a copy/symlink target exists; rejects sync operations; exits non-zero when run from the root checkout. Declarative config can also enable strict mode with top-level `strict = true`. For doctor, strict failures are reported as fatal diagnostics when possible. |
| `-f`, `--force`                                            | run/copy/symlink/sync                                         | Replaces existing file-operation targets where supported.                                                                                                                                                                                                                   |
| `-n`, `--dry-run`                                          | run/teardown/copy/symlink/sync                                | Prints planned work without writing files or running commands. Teardown does not prompt or require `--yes`.                                                                                                                                                                 |
| `-v`, `--verbose`                                          | run/copy/symlink/sync                                         | Prints detailed file-operation actions instead of compact summaries. Interactive progress is disabled in verbose mode.                                                                                                                                                      |
| `--skip-commands`                                          | run                                                           | Runs file operations only.                                                                                                                                                                                                                                                  |
| `--yes`                                                    | teardown                                                      | Long-only explicit approval that suppresses the terminal confirmation prompt. It does not bypass discovery, parsing, or validation.                                                                                                                                         |
| `-t`, `--target <path>`                                    | copy/symlink/sync                                             | Overrides the target. With multiple sources, acts as the target path prefix for each source.                                                                                                                                                                                |
| `--required`                                               | copy/symlink/sync                                             | Fails when any requested source does not exist.                                                                                                                                                                                                                             |
| `--symlinks <preserve>`                                    | copy/sync                                                     | Selects how source symlinks are handled. The initial supported value is `preserve`.                                                                                                                                                                                         |
| `--include <pattern>`                                      | copy/sync                                                     | Repeats to narrow directory operations to source paths matching operation-local include patterns. Patterns use gitignore-style syntax without `!` negation and are not read from `.gitignore` files. Conflicts with `--delete` / `-D`.                                      |
| `--ignore <pattern>`                                       | copy/sync                                                     | Repeats to skip source paths matching operation-local ignore patterns. Patterns use gitignore-style syntax and are not read from `.gitignore` files.                                                                                                                        |
| `--ignore-metadata <permissions\|owner\|group\|ownership>` | copy/sync                                                     | Repeats to opt out of metadata comparison and preservation. `ownership` means owner and group.                                                                                                                                                                              |
| `--compare <metadata\|checksum>`                           | sync                                                          | Selects sync comparison behavior.                                                                                                                                                                                                                                           |
| `-D`, `--delete` / `--no-delete`                           | sync                                                          | Controls whether sync deletes target-only files. Defaults to `--no-delete`.                                                                                                                                                                                                 |
| `--config`                                                 | init                                                          | Creates a starter TOML config. This intentionally has no short alias so `-c` can consistently mean config path for run/teardown/config.                                                                                                                                     |
| `-p`, `--path <path>`                                      | init                                                          | Writes the generated init output to a custom path.                                                                                                                                                                                                                          |

## Structured output formats

Commands that accept `--format json`, `--json`, `--format yaml`, or `--yaml`
must emit the structures in this section. JSON output is pretty-printed and YAML
output uses the same field names, values, and nesting as JSON. Path values are
strings. Optional values are `null` when absent. Tagged enum values are
lowercase `snake_case` strings. JSON object member order is not part of the
contract.

The shared worktree context object has this shape:

```json
{
  "root_path": "/repo",
  "worktree_path": "/repo-worktree",
  "default_branch": "main"
}
```

### `treeboot status` JSON

`treeboot status`, and its `treeboot info` alias, emit a discovery report:

```json
{
  "context": {
    "root_path": "/repo",
    "worktree_path": "/repo-worktree",
    "default_branch": "main"
  },
  "config": "/repo-worktree/.treeboot.toml"
}
```

`config` is a path string or `null`.

### `treeboot version` JSON

`treeboot version` emits package and implemented-spec metadata:

```json
{
  "package": "treeboot",
  "version": "0.8.0",
  "spec_version": "2.1.0"
}
```

`package` is the CLI package name. `version` is the package version.
`spec_version` is the TreeBoot spec version implemented by the build.

### `treeboot config` JSON

`treeboot config` emits the selected config path and normalized config:

```json
{
  "path": "/repo-worktree/.treeboot.toml",
  "config": {
    "strict": false,
    "default_ignore": [],
    "dangerously_allow_sources_outside_root": false,
    "dangerously_allow_targets_outside_worktree": false,
    "files": [
      {
        "operation": "copy",
        "source": ".env",
        "target": ".env",
        "source_path": "/repo/.env",
        "target_path": "/repo-worktree/.env",
        "required": false,
        "compare": null,
        "delete": null,
        "symlinks": "preserve",
        "include": [],
        "ignore": [],
        "ignore_metadata": [],
        "declaration": {
          "start": 0,
          "end": 15,
          "line": 1,
          "column": 1
        }
      }
    ],
    "commands": [
      {
        "name": "Install packages",
        "command": {
          "kind": "shell",
          "run": "mise install"
        },
        "cwd": null,
        "cwd_path": null,
        "env": {},
        "allow_failure": false,
        "declaration": {
          "start": 17,
          "end": 50,
          "line": 3,
          "column": 1
        }
      }
    ],
    "teardown_commands": [
      {
        "name": "Drop database",
        "command": {
          "kind": "shell",
          "run": "mise run db:drop"
        },
        "cwd": null,
        "cwd_path": null,
        "env": {},
        "allow_failure": false,
        "declaration": {
          "start": 52,
          "end": 98,
          "line": 5,
          "column": 1
        }
      }
    ]
  }
}
```

`files`, `commands`, and `teardown_commands` are ordered arrays. Omitted
collections normalize to empty arrays. File `operation` is `copy`, `symlink`, or
`sync`. `compare` is `metadata`, `checksum`, or `null`. `delete` is a boolean or
`null`. `symlinks` is `preserve` or `null`. `include` is an ordered array of
operation-local path include patterns. `ignore` is an ordered array of
operation-local path ignore patterns. `ignore_metadata` is an ordered array of
canonical ignored metadata fields: `permissions`, `owner`, and `group`. Config
input can use `ownership` as a shorthand, but normalized inspection output
expands it to `owner` and `group`.

Command `name`, `cwd`, and `cwd_path` are strings or `null`. `env` is an object
whose keys and values are strings. `command` is one of:

```json
{
  "kind": "shell",
  "run": "mise install"
}
```

```json
{
  "kind": "direct",
  "program": "npm",
  "args": [
    "install"
  ]
}
```

Each `declaration` object describes the byte and one-based line/column location
of the source TOML declaration:

```json
{
  "start": 0,
  "end": 15,
  "line": 1,
  "column": 1
}
```

### `treeboot check` JSON

`treeboot check` emits the resolved context, the selected bootstrap action, and
non-fatal run-validation warnings:

```json
{
  "context": {
    "root_path": "/repo",
    "worktree_path": "/repo-worktree",
    "default_branch": "main"
  },
  "action": {
    "kind": "config",
    "path": "/repo-worktree/.treeboot.toml"
  },
  "warnings": []
}
```

`warnings` is an ordered array of human-readable warning strings produced by run
validation, such as an include list that matches no source paths. It is empty
when validation produces no warnings.

`action` is one of:

```json
{ "kind": "missing_config" }
```

```json
{ "kind": "root_worktree_skipped" }
```

```json
{
  "kind": "config",
  "path": "/repo-worktree/.treeboot.toml"
}
```

### `treeboot doctor` JSON

`treeboot doctor` emits an ordered diagnostics report:

```json
{
  "fatal": false,
  "context": {
    "root_path": "/repo",
    "worktree_path": "/repo-worktree",
    "default_branch": "main"
  },
  "diagnostics": [
    {
      "name": "worktree",
      "status": "ok",
      "message": "worktree context resolved"
    },
    {
      "name": "config",
      "status": "ok",
      "message": "config is valid: /repo-worktree/.treeboot.toml"
    },
    {
      "name": "teardown_validation",
      "status": "ok",
      "message": "teardown config is valid"
    }
  ]
}
```

`fatal` is `true` when any diagnostic is fatal. `context` is the shared context
object or `null` when context discovery fails. Each diagnostic has a stable
`name`, a `status` of `ok`, `warning`, or `error`, and a human-readable
`message`.

The diagnostic names defined by this spec are `environment_options`, `worktree`,
`root`, `root_worktree`, `default_branch`, `environment`, `config`,
`config_validation`, and `teardown_validation`. Bootstrap validation preserves
the spec-2.0 behavior: success uses `config` with `config is valid: <path>`,
while failure uses `config_validation`. Teardown validation is reported
separately as `teardown_validation`.

The `default_branch` diagnostic is `ok` when a non-empty default branch was
resolved and `warning` when default branch discovery falls back to the
best-effort empty string. An unknown default branch is not fatal; treeboot still
sets `TREEBOOT_DEFAULT_BRANCH` and `CONDUCTOR_DEFAULT_BRANCH` to an empty
string.

### `treeboot env` JSON

`treeboot env` emits an object containing only treeboot-owned child environment
variables:

```json
{
  "CODEX_SOURCE_TREE_PATH": "/repo",
  "CODEX_WORKTREE_PATH": "/repo-worktree",
  "CONDUCTOR_DEFAULT_BRANCH": "main",
  "CONDUCTOR_ROOT_PATH": "/repo",
  "CONDUCTOR_WORKSPACE_PATH": "/repo-worktree",
  "GIT_SOURCE_TREE_PATH": "/repo",
  "GIT_WORKTREE_PATH": "/repo-worktree",
  "SUPERSET_ROOT_PATH": "/repo",
  "TREEBOOT_DEFAULT_BRANCH": "main",
  "TREEBOOT_ROOT_PATH": "/repo",
  "TREEBOOT_WORKTREE_PATH": "/repo-worktree"
}
```

Keys are variable names and values are strings. The object excludes the parent
process environment, per-command config overlays, and config option override
variables that treeboot reads from the parent environment.

### `treeboot schema` JSON

`treeboot schema` emits the bundled config JSON Schema document directly. It is
not wrapped in a treeboot report object and it does not support the structured
output flags. The schema payload is defined by `schemas/treeboot.schema.json`.

### Commands without structured output

`treeboot run`, `treeboot teardown`, `treeboot init`, `treeboot copy`,
`treeboot symlink`, `treeboot sync`, and `treeboot completions` do not support
`--format`, `--json`, or `--yaml`. Their output is text-only and follows the
command sections plus
[Operator experience](#operator-experience-output-and-exit-codes).

## Path model: Root path feeds the worktree path

User-facing docs use "root path" instead of "main worktree" to avoid confusion
with main branches. The spec still explains that Git's main worktree is the
default source when no override exists.

### Root path

Source checkout treeboot reads from. Defaults to Git's main worktree, but can be
overridden.

### Worktree path

Current worktree root. File targets and command execution are anchored here.

The current worktree path resolved from `git rev-parse --show-toplevel`
preserves platform-native filesystem path data. On Unix, this includes non-UTF-8
bytes and boundary whitespace. Only Git's single trailing LF delimiter is
removed.

### Root Path Discovery

The root path is the checkout used as the source for copy, symlink, and sync
operations.

1. Use `--root`, if provided.
2. Use `TREEBOOT_ROOT_PATH`, if set.
3. Use `CODEX_SOURCE_TREE_PATH`, if set.
4. Use `CONDUCTOR_ROOT_PATH`, if set.
5. Use `SUPERSET_ROOT_PATH`, if set.
6. Use Git's main worktree from `git worktree list --porcelain -z`.

Git-discovered worktree and root paths preserve platform-native filesystem path
data. On Unix, this includes non-UTF-8 bytes and boundary whitespace. Git's
NUL-delimited worktree output is parsed as bytes so path quoting and embedded
newlines cannot change the discovered root path.

Treeboot always retains Git's discovered main-worktree path as a separate
identity used for root-target checks. `--root` and environment aliases override
the file-operation source root only; they cannot make the actual main worktree
look like a linked worktree.

If no root path can be determined, `treeboot run` fails with a clear error.

### Root Checkout No-op

If Git's actual main-worktree identity and the current worktree path are the
same, `treeboot run` is running from the root checkout rather than a separate
worktree. This classification is unchanged by source-root overrides.

In the default mode, treeboot prints `This is not a work tree` and exits
successfully before discovering config. In strict mode enabled by `--strict` or
`TREEBOOT_STRICT`, treeboot prints the same info message and exits non-zero.
Config-level `strict` is not loaded before this check.

The same root-checkout behavior applies to manual `copy`, `symlink`, and `sync`
commands. In default mode they exit successfully without applying file
operations. In strict mode they exit non-zero before applying file operations.
`treeboot doctor` reports root-checkout strictness as a fatal `root_worktree`
diagnostic instead of exiting before printing the diagnostics report.

`treeboot teardown` has no root-checkout no-op. It rejects the main/root
checkout with exit code `1` before loading config or reading confirmation input.
Teardown is only valid for a linked worktree. Bootstrap strictness does not
change this rule.

### Default Branch Discovery

`TREEBOOT_DEFAULT_BRANCH` is best effort. treeboot uses an existing
`CONDUCTOR_DEFAULT_BRANCH` value if present, otherwise resolves `origin/HEAD`,
otherwise sets an empty string. When resolved from Git, the value is the short
branch name, such as `main`, not `origin/main` or a full ref path.

## Compatibility: Environment variables

Configured commands adapted from Codex, Conductor, or Superset setup scripts
should usually work with minimal changes. treeboot sets canonical variables plus
aliases for common setup-script ecosystems.

### Scope

treeboot builds one environment variable set for the resolved worktree and
applies it when executing bootstrap or teardown commands from declarative
config.

### Config option environment overrides

Bootstrap and manual file-operation flows read environment variables for
config-level boolean options. Values `1`, `true`, `yes`, and `on` enable an
option; `0`, `false`, `no`, and `off` disable it. Invalid values are errors
before bootstrap file operations or commands run.

```text
TREEBOOT_STRICT
TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT
TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE
```

These variables affect bootstrap file planning and manual file operations.
Command-only teardown does not parse or resolve bootstrap runtime policy, so
even invalid values do not fail teardown. They do not make a missing discovered
teardown config fatal, permit teardown of the root checkout, or change teardown
command planning.

### Canonical

```text
TREEBOOT_ROOT_PATH
TREEBOOT_WORKTREE_PATH
TREEBOOT_DEFAULT_BRANCH
```

### Aliases

```text
GIT_SOURCE_TREE_PATH
GIT_WORKTREE_PATH

CODEX_SOURCE_TREE_PATH
CODEX_WORKTREE_PATH

CONDUCTOR_ROOT_PATH
CONDUCTOR_WORKSPACE_PATH
CONDUCTOR_DEFAULT_BRANCH

SUPERSET_ROOT_PATH
```

| Variable                   | Value                     | Reason                                  |
| -------------------------- | ------------------------- | --------------------------------------- |
| `CODEX_SOURCE_TREE_PATH`   | `TREEBOOT_ROOT_PATH`      | Matches Codex setup-script terminology. |
| `CODEX_WORKTREE_PATH`      | `TREEBOOT_WORKTREE_PATH`  | Matches Codex setup-script terminology. |
| `GIT_SOURCE_TREE_PATH`     | `TREEBOOT_ROOT_PATH`      | Generic compatibility alias.            |
| `GIT_WORKTREE_PATH`        | `TREEBOOT_WORKTREE_PATH`  | Generic compatibility alias.            |
| `CONDUCTOR_ROOT_PATH`      | `TREEBOOT_ROOT_PATH`      | Supports Conductor-style setup scripts. |
| `CONDUCTOR_WORKSPACE_PATH` | `TREEBOOT_WORKTREE_PATH`  | Supports Conductor-style setup scripts. |
| `CONDUCTOR_DEFAULT_BRANCH` | `TREEBOOT_DEFAULT_BRANCH` | Supports Conductor-style setup scripts. |
| `SUPERSET_ROOT_PATH`       | `TREEBOOT_ROOT_PATH`      | Supports Superset-style setup scripts.  |

### Tool-mode variables are intentionally absent

treeboot does not set `CONDUCTOR_IS_LOCAL`, `CONDUCTOR_PORT`, or
`SUPERSET_PORT_BASE`. Those variables are owned by the tools that define them
and should not be fabricated.

## Execution: Run flow

Declarative config is the only bootstrap mode.

1. **Confirm Git context**: Fail early if not inside a Git working tree.
2. **Discover paths**: Resolve worktree path, root path, and default branch.
3. **Skip root checkout**: If Git's actual main-worktree identity and the
   current worktree path match, print `This is not a work tree` and exit before
   config; strict mode from CLI or environment exits non-zero.
4. **Build environment**: Set treeboot canonical variables and compatibility
   aliases.
5. **Load config**: Discover TOML config unless a specific path is provided. If
   no config is found, print an info message and exit.
6. **Resolve config options**: Merge top-level config options with environment
   overrides and CLI flags.
7. **Validate config**: Normalize entries and detect duplicate operation
   targets.
8. **Apply files, then commands**: Run file operations first; commands run
   afterward.

## Execution: Teardown flow

Teardown is an explicitly approved, command-only phase for a linked worktree.

1. **Select target**: Resolve `--worktree`, or the process working directory,
   through Git worktree discovery to a canonical worktree top level.
2. **Discover paths**: Resolve the root checkout, default branch, and
   treeboot-owned environment for that target.
3. **Reject root checkout**: Fail before config loading or confirmation when
   Git's actual main-worktree identity and target worktree path are equal.
   Explicit and environment source-root overrides do not change this identity.
4. **Load config**: Discover TOML config in the target worktree, or use only the
   explicitly requested config path. Report `treeboot: config detected <path>`
   after selecting the path and before reading or parsing it.
5. **Normalize the whole document**: Parse all declarations and construct one
   normalized config. Any TOML syntax, type, unknown-field, declaration-shape,
   or declaration-normalization error anywhere in the document is fatal.
6. **Plan teardown**: Validate only the teardown command collection. Bootstrap
   file and command semantic planning failures do not prevent a valid teardown.
7. **Approve**: Skip approval for no-op and dry-run outcomes. Otherwise require
   `--yes` or affirmative terminal confirmation.
8. **Execute the prepared plan**: Run the exact validated command set that was
   approved. Do not reload or reparse config after confirmation.

The preparation result distinguishes missing discovered config, a valid config
with no teardown commands, and a ready teardown plan. Missing discovered config
prints `treeboot: no config detected` and exits successfully. An explicitly
requested missing config is an error. An empty teardown plan prints
`treeboot: no teardown commands configured` and exits successfully. Neither
no-op prompts.

### Teardown confirmation

Confirmation occurs only after discovery, parsing, normalization, and teardown
planning succeed. The prompt is written to stderr and flushed before reading
stdin:

```text
Run 2 teardown commands for /repo/worktrees/feature?
These commands may delete resources outside the worktree. [y/N]
```

Only `y` and `yes`, case-insensitively after trimming whitespace, approve. Every
other response, an empty line, and EOF decline. A decline runs nothing and exits
with code `1`.

The prompt uses `1 teardown command` for one command and
`<count> teardown commands` for every other count.

When stdin is not a terminal, teardown without `--yes` runs nothing, exits with
code `1`, and tells the caller to rerun with `--yes`. Prompt read or write
failures also run nothing and exit with code `1`. `--yes` suppresses the prompt
but does not bypass discovery, parsing, normalization, or validation.
`--dry-run` never prompts and never requires `--yes`.

The unsuccessful decline contract makes this composition safe:

```sh
treeboot teardown --worktree "$path" &&
  git worktree remove "$path"
```

Treeboot itself never performs the removal.

## Migration from legacy init scripts

treeboot does not discover or execute `.treeboot.sh`, `.treebootrc`, or
`.config/treeboot/init`. These names have no special meaning and are treated as
ordinary repository files, regardless of executable permissions.
`--no-init-script`, `init --script`, and `init -s` are not accepted options;
using them is a CLI usage error with exit code `2`.

Move custom setup into a declarative command. For example:

```toml
commands = [
  { run = "./scripts/bootstrap-worktree.sh" },
]
```

Configured commands inherit the complete `TREEBOOT_*` environment and
compatibility aliases and run from the worktree root by default. Unlike the old
init-script invocation, treeboot does not automatically pass the root checkout
as positional argument `$1`; scripts should read `TREEBOOT_ROOT_PATH`, or the
config should pass it explicitly. Configured commands run after configured file
operations. `--skip-commands` omits them, and `--dry-run` reports them without
executing them.

## Declarative mode: Config files

TOML is the intended config format. Simple lists cover the common case;
top-level options and object entries cover stricter runtime, file, and command
behavior.

### Discovery

```text
.treeboot.toml
treeboot.toml
.config/treeboot/config.toml
```

The first existing config file wins. If `--config` is provided, only that file
is used. Relative config paths resolve from `TREEBOOT_WORKTREE_PATH`.

### Init Defaults

`treeboot init` creates `.treeboot.toml` by default. `treeboot init --config` is
an explicit spelling of the same config output. The command never prompts
interactively and fails if the output path already exists, including when that
path is a symlink. The starter config includes an empty `teardown_commands = []`
declaration.

```toml
teardown_commands = [
]
```

### Missing config

If no config file is detected, treeboot prints an info message such as
`treeboot: no config detected`. Without `--strict` or `TREEBOOT_STRICT` it exits
successfully. With either one, it exits non-zero. Config-level `strict` cannot
affect missing-config behavior because no config has been loaded.

For `treeboot teardown`, missing discovered config is always a successful no-op.
Teardown does not resolve bootstrap strictness. An explicitly selected missing
config remains an error.

### Config inspection

`treeboot config` uses the same config discovery rules as run mode, parses the
selected TOML file, normalizes file, bootstrap command, and teardown command
declarations, and exits successfully when parsing and normalization succeed. It
does not apply file operations or execute configured commands. Invalid TOML,
unknown fields, invalid enum values, missing required fields, and mutually
exclusive command fields are config errors.

After normalization, config inspection independently validates bootstrap and
teardown semantics. It prints phase-labelled `treeboot: warning: ...` lines for
each invalid phase without changing the successful parse-only exit status.
Non-fatal bootstrap validation warnings, such as an include list that matches no
source paths, are printed the same way. Warnings go to stderr in every output
format, so JSON and YAML stdout output stays parseable and the structured config
shape is unchanged.

### JSON Schema

The checked-in JSON Schema for the config file format lives at
`schemas/treeboot.schema.json`. It is generated from the Rust schema model with
`mise run generate` and checked in CI with `mise run generate:schema:check`
through the aggregate `mise run generate:check` task.

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
  { source = "shared/.agents", target = ".agents" },
]

sync = [
  "shared/config",
  { source = "tooling/config", target = ".config/tooling" },
]

commands = [
  "mise install",
  { name = "Install dependencies", run = "mise run setup" },
]

teardown_commands = [
  { name = "Stop services", run = "docker compose down" },
  { name = "Drop database", run = "mise run db:drop" },
]

files = [
  { operation = "copy", source = ".npmrc", target = ".npmrc" },
  { operation = "symlink", source = "shared/bin", target = "bin" },
  { operation = "sync", source = "shared/editor", target = ".editor" },
]
```

### Top-level options

Top-level options are project defaults for declarative bootstrap execution.
Environment variables override matching config values. CLI flags override both
where an equivalent flag exists. The normalized config still carries these
values during teardown, but command-only teardown does not resolve or apply
them.

| Option                                       | Environment                                           | Meaning                                                                                                                                                 |
| -------------------------------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `strict`                                     | `TREEBOOT_STRICT`                                     | Defaults to `false`. Enables stricter declarative validation and conflict handling. CLI or environment strictness also applies before config discovery. |
| `dangerously_allow_sources_outside_root`     | `TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT`     | Defaults to `false`. Allows declarative file operation sources outside `TREEBOOT_ROOT_PATH`.                                                            |
| `dangerously_allow_targets_outside_worktree` | `TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE` | Defaults to `false`. Allows declarative file operation targets outside `TREEBOOT_WORKTREE_PATH`.                                                        |
| `default_ignore`                             | none                                                  | Defaults to `[]`. Ordered path ignore patterns prepended to every `copy` and `sync` operation's effective ignore list.                                  |

### File objects

`copy`, `symlink`, and `sync` accept strings and objects. Strings mean source
and target are the same path. For `sync`, string entries also use
`compare = "metadata"` and `delete = false`. The `files` list accepts objects
with an `operation` field for mixed copy, symlink, and sync entries. Missing
sources are skipped by default; object entries can set `required = true` to make
a missing source fail. When an object has `source` but no `target`, its target
defaults to the same path as `source`.

```toml
copy = [
  ".env.local",
  { source = ".env.development.local" },
  ".env.test.local",
  "mise.local.toml",
  { source = ".env.required.local", required = true },
]

symlink = [
  ".tool-versions",
  { source = "shared/.agents", target = ".agents" },
]

sync = [
  "shared/config",
  { source = "tooling/config", target = ".config/tooling", delete = true },
  { source = "shared/tool.lock", target = ".tool.lock", compare = "checksum" },
  { source = "shared/cache", target = ".cache/shared", ignore_metadata = ["ownership"] },
  { source = "shared/vendor", ignore = ["**/tmp/**", "!**/tmp/keep/**"] },
]

files = [
  { operation = "copy", source = ".npmrc", target = ".npmrc" },
  { operation = "copy", source = ".env.local", ignore_metadata = ["permissions"] },
  { operation = "symlink", source = "shared/bin", target = "bin" },
  { operation = "sync", source = "shared/editor", target = ".editor" },
]

[[file]]
operation = "copy"
source = "templates/editorconfig"
target = ".editorconfig"
```

The verbose table-array name is singular `[[file]]` so it can coexist with the
plural `files = [...]` list in the same TOML file.

| File field        | Applies to          | Meaning                                                                                                                                                                      |
| ----------------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `operation`       | `files`, `[[file]]` | Required for object entries; one of copy, symlink, or sync.                                                                                                                  |
| `source`          | all file operations | Required for object entries. Relative paths resolve from root path.                                                                                                          |
| `required`        | all file operations | Defaults to `false`. When true, a missing source is a failure instead of a skipped operation.                                                                                |
| `target`          | all file operations | Optional; defaults to source. Relative paths resolve from worktree path.                                                                                                     |
| `compare`         | `sync`              | `metadata` by default; `checksum` for content checks.                                                                                                                        |
| `delete`          | `sync` directories  | Defaults to `false`; when true, deletes target-only files and directories.                                                                                                   |
| `symlinks`        | `copy`, `sync`      | Defaults to `preserve`; safe source symlinks are recreated as symlinks and unsafe symlinks are validation errors.                                                            |
| `include`         | `copy`, `sync`      | Optional list of operation-local path include patterns that narrow directory operations to matching source paths. A non-empty list cannot combine with sync `delete = true`. |
| `ignore`          | `copy`, `sync`      | Optional list of operation-local path ignore patterns appended after top-level `default_ignore`.                                                                             |
| `ignore_metadata` | `copy`, `sync`      | Optional list of metadata fields to ignore. Supported values are `permissions`, `owner`, `group`, and `ownership`. `ownership` is shorthand for owner and group.             |

### Command objects

`commands` accepts string entries and compact object entries. Strings are
shorthand for objects with a `run` field. For longer command definitions, use
verbose `[[command]]` entries.

`teardown_commands` and verbose `[[teardown_command]]` entries accept the same
fields and defaults. Compact teardown entries run before verbose teardown
entries, matching the bootstrap `commands` then `[[command]]` ordering.
Bootstrap and teardown normalize into separate ordered collections. An omitted
teardown declaration normalizes to an empty collection, so existing config files
remain valid. Older treeboot versions reject configs using the new keys because
unknown config fields are deliberately denied.

```toml
commands = [
  "mise install",
  { run = "mise run setup", env = { NODE_ENV = "development" } },
]

teardown_commands = [
  { name = "Stop services", run = "docker compose down" },
]

[[command]]
name = "Install dependencies"
run = "npm install"
cwd = "."
allow_failure = false
[command.env]
NODE_ENV = "development"

[[command]]
name = "Install dependencies without a shell"
program = "npm"
args = ["install"]

[[teardown_command]]
name = "Drop database"
program = "mise"
args = ["run", "db:drop"]
allow_failure = false
```

| Command field      | Meaning                                                                                                                     |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| `run`              | Shell command to execute. Mutually exclusive with `program`.                                                                |
| `program` / `args` | Direct process execution without shell parsing.                                                                             |
| `cwd`              | Command working directory, relative to the worktree by default. Normalized paths must stay inside `TREEBOOT_WORKTREE_PATH`. |
| `env`              | Extra environment variables merged into the treeboot env set. Treeboot-owned variables and aliases cannot be overridden.    |
| `allow_failure`    | Defaults to `false`. When true, non-zero exit is not fatal.                                                                 |

Exactly one of `run` or `program` is required. `args` is valid only with
`program`. The parser rejects unknown fields in both bootstrap and teardown
command declarations.

## Before execution: Operation validation

treeboot should catch surprising or self-conflicting file operations before it
starts changing the worktree.

### Whole-document and phase boundary

Config loading parses and normalizes the whole TOML document before any
bootstrap or teardown semantic plan is built. A syntax, type, unknown-field,
declaration-shape, mutually exclusive field, or path-normalization error in any
declaration makes config loading fail for every command.

After a normalized config exists, bootstrap and teardown semantic planning are
independent:

- `treeboot run` validates and builds only the bootstrap plan.
- `treeboot teardown` validates and builds only the teardown plan.
- a missing required bootstrap source, strict sync conflict, bootstrap command
  cwd escape, or bootstrap owned-environment override does not block valid
  teardown planning
- an invalid teardown cwd or owned-environment override does not block bootstrap
  planning
- `treeboot check`, `treeboot doctor`, and `treeboot config` independently
  evaluate both phase outcomes from one complete-config validation pass

Complete-config validation orders bootstrap before teardown deterministically
and preserves both errors when both phases fail. `check` returns one fatal
aggregate error containing every failed phase. `doctor` reports separate phase
diagnostics. `config` reports phase-labelled warnings while preserving its
successful parse-only status.

### Normalize first

Config parsing should normalize `copy`, `symlink`, `sync`, `files`, and
`[[file]]` into one ordered list of file operations with resolved source and
target paths. It normalizes bootstrap and teardown declarations into separate
ordered command collections. Manual `copy`, `symlink`, and `sync` commands
should produce the same normalized operation shape.

Relative file-operation source paths resolve from `TREEBOOT_ROOT_PATH`. Relative
file-operation target paths and command `cwd` paths resolve from
`TREEBOOT_WORKTREE_PATH`. Absolute paths are preserved and then normalized
against the filesystem where possible. Missing final path components are
normalized by canonicalizing the nearest existing ancestor and applying lexical
`.` / `..` cleanup to the missing tail.

On Windows, normalized paths must be emitted in ordinary tool-friendly form when
the platform returns an extended-length path during canonicalization. Fully
qualified drive paths such as `C:\repo\file`, UNC paths such as
`\\server\share\repo\file`, and relative paths without a drive or root prefix
are supported. Drive-relative paths such as `C:repo\file` and root-relative
paths without a drive or share such as `\repo\file` are rejected for declarative
file-operation paths, manual file-operation paths, and command `cwd` values
because they depend on process-local current-drive state.

### Conflicting targets

If multiple file operations target the same normalized absolute path, or one
target is inside another target, treeboot should report every conflicting entry
with its operation, source, target, and declaration location when available.

### Target boundary

Every file operation target must resolve inside `TREEBOOT_WORKTREE_PATH`.
Targets outside the current worktree are validation errors by default. For
symlink operations, the target is the path of the symlink created in the
worktree; the symlink destination is the source and may point back into
`TREEBOOT_ROOT_PATH`.

Existing target parent components under `TREEBOOT_WORKTREE_PATH` must be real
directories. If a target parent component is a symlink or another non-directory
file type, validation fails before any file operation or command runs. Missing
parent components are allowed and created as directories when the operation is
applied. The final target path itself is not treated as a parent component:
force and sync may still replace the final target path when the conflict matrix
allows replacing an existing file or symlink.

Immediately before a file operation mutates a target, treeboot must re-check the
live target ancestor chain. If a target ancestor became a symlink or
non-directory after validation, that operation fails before mutating the target.

### Source boundary

Every file operation source must resolve inside `TREEBOOT_ROOT_PATH`. Sources
outside the root path are validation errors by default.

### Command boundary

Bootstrap and teardown command `cwd` values are normalized relative to
`TREEBOOT_WORKTREE_PATH`. Paths may contain `..`, but the final resolved path
must stay inside the worktree. Planning resolves each cwd before any side effect
and retains that canonical worktree boundary. Immediately before spawning each
command, treeboot re-resolves the cwd and live worktree root. The live root must
still equal the planning-time boundary, and the freshly resolved cwd must remain
inside it. For bootstrap this happens after file operations; teardown has no
file phase. A changed root or escaped cwd prevents the spawn and follows the
normal command-start `allow_failure` policy.

| Rule                                                        | Behavior                                                        |
| ----------------------------------------------------------- | --------------------------------------------------------------- |
| Any duplicate operation target                              | Fail before any file operation or command runs.                 |
| Target resolves outside the worktree                        | Fail before any file operation or command runs.                 |
| Target parent is a symlink or non-directory at validation   | Fail before any file operation or command runs.                 |
| Target ancestor becomes a symlink or non-directory at apply | Fail that operation before mutating the target.                 |
| Source resolves outside the root path                       | Fail before any file operation or command runs.                 |
| Required file operation source does not exist               | Fail before any file operation or command runs.                 |
| Optional file operation source does not exist               | Skip that operation, make no target changes, and continue.      |
| Command `cwd` resolves outside the worktree during planning | Fail before any file operation or command runs.                 |
| Live worktree root differs from its planned boundary        | Do not spawn it; fail or warn and continue per `allow_failure`. |
| Command `cwd` resolves outside the worktree before spawn    | Do not spawn it; fail or warn and continue per `allow_failure`. |
| Command `env` overrides treeboot-owned variables            | Fail before any file operation or command runs.                 |
| Copy or sync encounters an unsafe source symlink            | Fail before any file operation or command runs.                 |
| Preserved copy or sync source symlink changes before apply  | Fail that operation before mutating the target.                 |
| Strict mode with any sync operation                         | Fail before any file operation or command runs.                 |
| `--dry-run`                                                 | Print the validation error, change no files, and exit non-zero. |

### Conflicting targets are invalid config

A config that copies a file and later symlinks to the same target is ambiguous
at best and destructive under force mode. treeboot should reject duplicate
configured targets in every mode. It should also reject ancestor/descendant
target pairs because a sync operation with `delete = true` can remove
target-only children produced by another operation in the same plan. Manual
commands should reject duplicate and overlapping targets derived from their
source arguments and `--target` before any file changes are made.

### Outside-worktree targets need an explicit escape hatch

The top-level option `dangerously_allow_targets_outside_worktree = true`
disables the target-boundary check. `dangerously_allow_sources_outside_root`
separately disables the source-boundary check. These checks affect declarative
file operations and manual file operation commands. Configured commands remain
unrestricted because treeboot cannot safely validate arbitrary command behavior.

### Strict mode is incompatible with sync

Sync expects existing targets and can be configured to delete target-only files,
so strict mode rejects sync operations before execution. Strict mode can be
enabled with `--strict`, top-level `strict = true`, or `TREEBOOT_STRICT=true`.

## Files first: File operations

Sources resolve against the root path. Targets resolve against the worktree
path. Parent target directories are created as needed.

| Config                                               | Source                                         | Target                                                     |
| ---------------------------------------------------- | ---------------------------------------------- | ---------------------------------------------------------- |
| `copy = [".env.local"]`                              | `TREEBOOT_ROOT_PATH/.env.local`                | `TREEBOOT_WORKTREE_PATH/.env.local`                        |
| `symlink = [".tool-versions"]`                       | `TREEBOOT_ROOT_PATH/.tool-versions`            | `TREEBOOT_WORKTREE_PATH/.tool-versions`                    |
| `sync = ["shared/config"]`                           | `TREEBOOT_ROOT_PATH/shared/config`             | `TREEBOOT_WORKTREE_PATH/shared/config`                     |
| `{ source = "a", target = "b" }`                     | `TREEBOOT_ROOT_PATH/a`                         | `TREEBOOT_WORKTREE_PATH/b`                                 |
| `{ operation = "sync", source = "a", target = "b" }` | `TREEBOOT_ROOT_PATH/a`                         | `TREEBOOT_WORKTREE_PATH/b`                                 |
| `treeboot copy a --target b`                         | `TREEBOOT_ROOT_PATH/a`                         | `TREEBOOT_WORKTREE_PATH/b`                                 |
| `treeboot copy a c --target b`                       | `TREEBOOT_ROOT_PATH/a`, `TREEBOOT_ROOT_PATH/c` | `TREEBOOT_WORKTREE_PATH/b/a`, `TREEBOOT_WORKTREE_PATH/b/c` |

### Manual operation source completion

Shell completions for the source arguments of `treeboot copy`,
`treeboot symlink`, and `treeboot sync` should list files and directories from
the resolved root path, not from the current worktree. Completion candidates
should be relative to the root path so completed values can be reused as default
targets.

Root-relative source completion is part of the completion contract for every
shell supported by `treeboot completions`: Bash, Zsh, Fish, PowerShell, and
Elvish.

Completion candidate generation uses root/worktree discovery only. It must not
parse config files, run configured commands, or fail because config is missing
or invalid.

### Manual operation normalization

Manual file operation commands normalize to the same internal file operation
shape as config entries. The subcommand supplies `operation`, each positional
source supplies `source`, `--required` supplies `required = true`, and
operation-specific flags supply `symlinks`, `compare`, `delete`, `include`,
`ignore`, or `ignore_metadata`.

Manual normalization happens under the same resolved runtime policy as
declarative file operations: defaults, then config top-level policy when a
config is present, then environment overrides, then CLI strictness. For manual
`copy` and `sync`, effective ignore rules are the loaded config's
`default_ignore` patterns followed by repeated `--ignore` flags. Effective
include rules are the repeated `--include` flags alone; no config default is
merged.

If `--target` is omitted, each target defaults to its source value. If one
source is passed, `--target` is that operation's target. If more than one source
is passed, `--target` is joined with each source value to produce each
operation's target.

### Missing sources

Missing sources are optional by default for copy, symlink, and sync. When a
source does not exist and the entry does not set `required = true`, treeboot
skips that operation and leaves the target unchanged. This lets one config list
several local-only files, such as `.env.local` and `.env.development.local`,
while only applying the files that exist in the root checkout.

### Copy

Copies files and directories. Directory copies recursively copy the source
directory into the configured target path. This is a copy operation, not a sync
operation: treeboot never deletes target files merely because they are absent
from the source. Source symlinks are preserved by default when they are safe. By
default, copy preserves the metadata described in
[File metadata preservation](#file-metadata-preservation). Configure `ignore`,
or use `--ignore`, to skip selected source paths during directory copies.
Configure `include`, or use `--include`, to narrow directory copies to selected
source paths. Configure `ignore_metadata`, or use `--ignore-metadata`, to opt
out of selected metadata fields.

`treeboot copy` exposes `--target`, `--required`, `--symlinks`, `--include`,
`--ignore`, `--ignore-metadata`, `--dry-run`, `--verbose`, `--strict`, and
`--force`.

### Symlink

Creates relative symlinks whenever treeboot can compute the path from the target
parent to the source. If it cannot, it falls back to an absolute symlink.

`treeboot symlink` exposes `--target`, `--required`, `--dry-run`, `--verbose`,
`--strict`, and `--force`.

### Sync

Reconciles target content to match source content. Files are compared by size
and modified time by default, or by content when `compare = "checksum"` is set.
Checksum comparison must detect content changes even when size and modified time
do not change. Sync also compares and repairs the metadata fields described in
[File metadata preservation](#file-metadata-preservation), unless those fields
are listed in `ignore_metadata`. Configure `ignore`, or use `--ignore`, to skip
selected source and target paths during directory sync. Configure `include`, or
use `--include`, to narrow directory sync to selected source paths; a non-empty
`include` list cannot be combined with `delete = true`. Source symlinks are
preserved by default when they are safe.

`treeboot sync` exposes `--target`, `--required`, `--compare`, `--delete`,
`--no-delete`, `--symlinks`, `--include`, `--ignore`, `--ignore-metadata`,
`--dry-run`, `--verbose`, `--strict`, and `--force`.

### Path ignore rules

`default_ignore` is an ordered top-level list of path patterns prepended to
every `copy` and `sync` operation's effective ignore list. `ignore` is an
ordered list of operation-local path patterns appended after `default_ignore`.
Patterns use gitignore-style syntax, including `*`, `?`, `**`, character
classes, trailing slash directory matches, comments, escaped metacharacters, and
`!` negation. Later matching patterns override earlier matching patterns. A path
matched by a non-negated pattern is ignored. A path matched by a later negated
pattern is re-included. Because operation-local `ignore` patterns come after
`default_ignore`, an operation-local `!` pattern can re-include a path ignored
by the top-level defaults.

A pattern that cannot be parsed as a glob is a file operation validation error,
reported before any side effects. Unlike Git, treeboot does not silently skip
malformed patterns.

Normalized file operations expose the effective merged ignore list in their
`ignore` field. Normalized config output also preserves `default_ignore` as a
top-level policy field.

treeboot never loads `.gitignore`, `.ignore`, `.rgignore`, `.git/info/exclude`,
or global Git ignore files for file operations. Ignore rules come only from
top-level `default_ignore`, the operation's `ignore` field, or repeated manual
`--ignore` flags.

Patterns match source-relative paths for the operation. For example, with
`source = "shared"` and `ignore = ["**/vendor/**"]`, the pattern is evaluated
against paths below `TREEBOOT_ROOT_PATH/shared`, not against paths below the
repository root unless the operation source is the root itself. Matching uses
directory knowledge, so directory-only patterns match only directories.

Ignore rules affect directory sources only. When a `copy` or `sync` source is a
single file or a source symlink, treeboot validates the patterns but does not
apply them to skip the top-level source. Use a directory source when selective
path filtering is required.

Ignored source paths are skipped before copy/sync action planning and before
unsafe source-symlink validation. Ignored unsafe symlinks are therefore not
validation errors. Re-included paths are planned and validated normally.

When negated patterns are present, treeboot must still be able to discover
re-included descendants under ignored directories. Implementations may traverse
ignored directories conservatively to find re-included descendants. Ignored
directories that exist only as ancestors of re-included descendants may be
created as target parent directories, but treeboot must not report or repair the
ignored directory itself unless that directory is re-included.

For directory sync with `delete = true`, ignore rules also apply to target-only
paths by evaluating the same operation-relative path under the sync target.
Ignored target-only files and directories are preserved. Re-included target-only
paths remain eligible for deletion.

### Path include rules

`include` is an ordered list of operation-local path patterns that narrows a
`copy` or `sync` operation to matching source paths. When an operation's include
list is empty or absent, no include filtering applies. When it is non-empty, a
path below a directory source is in scope only when it matches at least one
include pattern and is not ignored.

Include and ignore are independent gates evaluated against the same
source-relative paths. Include selects paths as a plain union: a path is
included when any include pattern matches it, without last-match-wins ordering.
Ignore rules then remove paths from that selection. A negated `!` ignore pattern
only unwinds earlier ignore patterns; it cannot re-include a path that does not
match the include rules.

Include patterns use the same gitignore-style syntax as ignore patterns, except
that every include entry must be an effective positive pattern. Patterns
beginning with `!`, blank or whitespace-only patterns, and `#` comment lines are
validation errors in include lists; exclusion belongs to `ignore`. The `\!` and
`\#` escapes remain valid and match literal leading `!` or `#` characters in
file names. As with ignore patterns, an include pattern that cannot be parsed as
a glob is a file operation validation error. Because inert entries are rejected,
a non-empty include list always filters.

A directory that matches an include pattern includes its whole subtree, subject
to ignore rules. A directory that matches no include pattern is still traversed
when a descendant could match. Directories receive target actions only when the
directory itself is included or its subtree contains an in-scope path; a path
that matches include but is ignored is not in scope and does not materialize its
ancestors. Ancestor directories of in-scope paths that are not ignored are
created as target parent directories and keep their normal metadata comparison
and repair behavior, even when every in-scope descendant is unchanged. Ignored
ancestors traversed only to reach in-scope descendants follow
[Path ignore rules](#path-ignore-rules) and are not reported or repaired unless
re-included. Directories with no in-scope descendants that are not themselves
included produce no target actions.

Like ignore rules, include rules affect directory sources only. When a `copy` or
`sync` source is a single file or a source symlink, treeboot validates the
patterns but never filters the top-level source. Each operation's include list
applies only within that operation's source tree; selecting which sources an
invocation covers is done by listing them.

Non-included source paths are skipped before copy/sync action planning and
before unsafe source-symlink validation. Non-included unsafe symlinks are
therefore not validation errors.

`include` is rejected on `symlink` operations, like `ignore`. A `sync` operation
with a non-empty `include` list cannot combine it with `delete = true`: config
validation rejects the combination, and `--include` conflicts with `--delete` /
`-D` for manual `treeboot sync`. Absent `include` and `include = []` do not
conflict with deletion. To mirror a subtree with deletion, narrow the
operation's source instead of using include patterns.

There is no top-level `default_include`. Include rules come only from the
operation's `include` field or repeated manual `--include` flags. For manual
`copy` and `sync`, the effective include list is the repeated `--include` flags
alone; the config `default_ignore` merging behavior has no include counterpart.

An include list that matches no source paths is not an error. `run` performs the
operation with no in-scope paths and stays silent. `treeboot check` and
`treeboot config` surface one non-fatal warning, in operation order, for each
`copy` or `sync` operation whose effective include list is non-empty, whose
source exists and is a directory, and whose source tree contains no path
matching the include rules before ignore filtering. Operations whose source is
missing, a single file, or a source symlink do not produce zero-match include
warnings.

Implementations may prune directories that cannot contain included paths instead
of traversing them. Because of this latitude, an unreadable directory that is
outside include scope may or may not produce an error. Pruning must not
otherwise change observable results.

Normalized file operations expose the operation-local include list in their
`include` field.

### Symlinks inside copy and sync

Copy and sync use `symlinks = "preserve"` by default: safe source symlinks are
recreated as symlinks instead of copying their referents. A symlink is unsafe if
it is empty or resolves outside `TREEBOOT_ROOT_PATH`. Preserved source symlinks
are rechecked immediately before target mutation; if the source stops being a
symlink, changes the planned target, or resolves outside `TREEBOOT_ROOT_PATH`,
treeboot fails the operation before creating or replacing the worktree link.
When source and target layouts differ, treeboot rewrites copied symlinks to
point at the analogous worktree destination when it can. Root-local symlink
targets are mapped by root-relative path into the worktree before treeboot
computes the destination symlink. When no rewrite is needed, treeboot preserves
the symlink target text. If the final symlink target does not exist and will not
be created by the current run, treeboot prints a warning. Unsafe symlinks are
validation errors in declarative config. Projects that need custom symlink
handling should use a configured command.

### File metadata preservation

Copy and sync preserve regular file contents, permissions, owner, group, and
modified time where the platform supports them. For directories, copy and sync
preserve permissions, owner, and group where supported. Directory modified time
is not preserved or compared because directory modified times change as children
are created, removed, or updated.

`ignore_metadata` lets a copy or sync operation opt out of selected metadata
comparison and preservation. Supported values are:

| Value         | Meaning                                                        |
| ------------- | -------------------------------------------------------------- |
| `permissions` | Do not compare or apply file or directory permission metadata. |
| `owner`       | Do not compare or apply owner metadata.                        |
| `group`       | Do not compare or apply group metadata.                        |
| `ownership`   | Shorthand for `owner` and `group`.                             |

Ignored metadata fields do not trigger sync updates and are not applied after
copy or sync content updates. Non-ignored metadata fields are still compared and
applied. Modified time is not configurable in this version; regular-file
modified time remains part of the default sync idempotency contract because
`compare = "metadata"` compares size and modified time for content drift.

Permission preservation failures are operation failures. Owner and group
preservation is best-effort when the operating system denies ownership changes,
because unprivileged users often cannot set arbitrary owners or groups. In that
case treeboot reports a warning and continues. Other unexpected ownership errors
are operation failures.

This metadata contract is intentionally narrower than archive copying. treeboot
does not preserve ACLs, extended attributes, resource forks, file flags,
hard-link identity, sparse-file layout, or other platform-specific archive
metadata. Projects that need archive semantics should use project-local commands
such as `rsync`, `cp -a`, `ditto`, or another purpose-built tool.

### Sync preserves extras by default

When the source is a directory, sync recurses through source and target. It
copies new files and updates changed files. Target files or directories that do
not exist in the source are preserved by default. Configure `delete = true` on a
sync entry, or use `--delete` / `-D` for manual sync, to delete target-only
files and directories. Deletion cannot be combined with a non-empty `include`
list; see [Path include rules](#path-include-rules).

### Operation order

treeboot executes `copy` list entries first, `symlink` list entries second,
`sync` list entries third, `files` entries fourth, then `[[file]]` entries in
document order.

## Safety: Conflict modes

The default mode is optimized for repeated worktree setup. Strict mode is for
CI-like validation. Force mode is intentionally destructive and should be
explicit.

### Trusted setup inputs

`treeboot run` and `treeboot teardown` are intended for repositories whose setup
contract the user trusts. The trust boundary includes declarative config files
and configured commands, which can run arbitrary code and may modify or delete
resources outside the worktree. Use `treeboot config` to inspect TOML without
execution, `treeboot run --skip-commands` to apply only configured file
operations, and `treeboot teardown --dry-run` to inspect teardown command
execution.

Dry-run reports the same file-operation decision that treeboot would take
without mutating files. Default text output reports one compact line per
top-level file operation when that operation has a visible decision. A single
file create, update, symlink, delete, or skip uses the same direct line shape as
the concrete action. Directory copy and sync operations summarize expanded child
actions with counts. `--verbose` reports each concrete child action instead of
the compact top-level summary.

The table below is the compatibility contract for file-operation conflicts.

| Case                                  | Default                                                                                              | `--strict`                             | `--force`                                | `--dry-run`                                                                                       |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------- | -------------------------------------- | ---------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Copy file to missing destination      | Create file and parents.                                                                             | Create file and parents.               | Create file and parents.                 | Report planned create.                                                                            |
| Copy file to existing file or symlink | Skip with info output.                                                                               | Fail before mutation.                  | Replace file or symlink.                 | Report skip, fail, or replace.                                                                    |
| Copy file to existing directory       | Fail operation.                                                                                      | Fail before mutation.                  | Fail; do not remove directory.           | Report failure.                                                                                   |
| Copy directory to missing destination | Recursively create directory tree. Summarize expanded child actions by default.                      | Recursively create directory tree.     | Recursively create directory tree.       | Report planned creates, summarized by default.                                                    |
| Copy directory to existing directory  | Recursively copy missing files and skip existing files. Summarize expanded child actions by default. | Fail before mutation.                  | Merge and overwrite matching files only. | Report planned creates, skips, fail, or merge, summarized by default.                             |
| Copy directory to file or symlink     | Fail operation.                                                                                      | Fail before mutation.                  | Fail; do not replace with directory.     | Report failure.                                                                                   |
| Symlink to missing destination        | Create parent directories and symlink.                                                               | Create parent directories and symlink. | Create parent directories and symlink.   | Report planned symlink.                                                                           |
| Symlink to existing file or symlink   | Skip with info output.                                                                               | Fail before mutation.                  | Replace file or symlink.                 | Report skip, fail, or replace.                                                                    |
| Symlink to existing directory         | Fail operation.                                                                                      | Fail before mutation.                  | Fail; do not remove directory.           | Report failure.                                                                                   |
| Sync file                             | Create or update when changed.                                                                       | Rejected by validation.                | Same as default.                         | Report create or update; stay silent when unchanged.                                              |
| Sync directory                        | Reconcile tree; preserve target-only files by default. Summarize expanded child actions by default.  | Rejected by validation.                | Same as default.                         | Report creates, updates, and explicit deletes, summarized by default; stay silent when unchanged. |
| Optional missing source               | Skip and leave target unchanged.                                                                     | Skip and leave target unchanged.       | Skip and leave target unchanged.         | Report planned skip.                                                                              |

### Directory copy under force

In force mode, copying a source directory over an existing target directory
traverses the source and overwrites matching target files. It does not remove
target files or directories that are not present in the source. Removing extras
would be sync behavior, not copy behavior.

### Force does not erase directories casually

Force mode may replace existing regular files and symlinks. It must not delete a
non-empty directory to satisfy a copy or symlink operation. Copying a file over
a directory, creating a symlink over a directory, or copying a directory over a
file are operation failures. Use sync when directory contents should be
reconciled.

### Sync is intentionally destructive

Sync is the operation that may delete target-only files when deletion is
explicitly enabled. Existing targets are expected for sync and are not treated
as conflicts in default or force mode. Strict mode rejects configs with sync
operations before runtime. Use `--dry-run` to preview sync creates, updates, and
explicit deletes.

## File operation output and progress

File-operation output should stay compact by default while preserving detailed
diagnostics when requested. Default text output is grouped by top-level file
operation. Top-level operations are declarative config entries or normalized
manual source arguments.

Single concrete actions omit parenthesized counts because the source and target
already describe the work:

```text
treeboot: copy .env -> .env
treeboot: sync .env -> .env
treeboot: would copy .env -> .env
treeboot: skip copy .env; target exists
treeboot: would skip copy .env; target exists
```

Expanded operations include counts after the source and target:

```text
treeboot: copy shared -> shared (12 changed)
treeboot: copy node_modules -> node_modules (1842 changed, 27 skipped)
treeboot: sync shared -> shared (4 changed, 1 deleted)
treeboot: would sync shared -> shared (4 changes, 1 delete)
```

Count words are singular when the count is one and plural otherwise. `changed`
counts created directories, copied files, created symlinks, and replaced files
or symlinks. `skipped` counts planned or actual skip decisions. `deleted` counts
sync target-only paths removed or planned for removal.

Manual multi-source commands report each normalized source operation
independently:

```text
treeboot copy a b --target local

treeboot: copy a -> local/a (12 changed)
treeboot: copy b -> local/b (4 changed, 2 skipped)
```

`--verbose` preserves the detailed action stream. In verbose mode, directory
copy and sync report concrete creates, updates, deletes, skips, and warnings
rather than only the grouped summary. Verbose mode disables interactive progress
rendering so detailed lines do not interleave with progress redraws.

When stdout and stderr are interactive terminals, non-verbose copy and sync
operations may render ephemeral progress on stderr: a spinner while planning a
top-level operation and a determinate progress bar while applying planned
actions. Progress rendering must not change the final stdout summary lines. When
output is redirected, captured by CI, or otherwise non-interactive, non-verbose
file operations must suppress spinner/progress control output and print only
normal summary, warning, command, and error lines.

File-operation warnings remain visible in compact mode. If a warning is emitted
while progress is active, progress must be cleared or suspended before printing
the warning so terminal output remains readable.

## Command phases: Shared runtime

Bootstrap and teardown commands are arbitrary project commands. treeboot plans
and runs both collections through the same command semantics while keeping the
phase plans separate.

### Execution rules

- Bootstrap commands run after file operations complete successfully and run
  even if every file operation was skipped.
- Teardown commands run without applying any file operation or bootstrap
  command.
- Commands run sequentially in declaration order.
- Re-resolve each command cwd and the live worktree root immediately before
  spawn, require the root to equal the planning-time boundary, and reject a live
  cwd escape.
- A command with `allow_failure = true` warns when it cannot be spawned or exits
  non-zero, then later commands continue.
- Run from `TREEBOOT_WORKTREE_PATH` unless a command sets `cwd`.
- Receive the full treeboot environment variable set described in
  [Environment variables](#compatibility-environment-variables).
- Per-command `env` values are merged into that environment for that command
  only.
- Bootstrap commands skip only when `treeboot run --skip-commands` is provided.
- In run `--dry-run`, report planned file operations and bootstrap commands
  without spawning any configured command process.
- In teardown `--dry-run`, report teardown commands without prompting or
  spawning any configured command process.

### Shells

```text
Unix:    sh -c <command>
Windows: cmd /C <command>

Direct:  program + args
```

String commands and objects with `run` use the shell. Objects with `program`
execute directly without shell parsing.

### Parallel setup work

treeboot does not parallelize configured commands. Projects that want parallel
setup work should delegate to one project-local task, such as `mise run setup`,
`make setup`, or another task runner command.

### Lifecycle output

For bootstrap, treeboot reports `treeboot: run <label>` before spawning each
command. In dry-run it reports `treeboot: would run <label>` instead.

For teardown, treeboot reports `treeboot: teardown run <label>` before spawning
each command. In dry-run it reports `treeboot: teardown would run <label>`
instead.

A successful command in either phase does not produce a separate success event.

### Command labels

Labels include both `name` and invocation when `name` is set, formatted as
`Name: invocation`. Without a name, shell commands use the shell string and
direct commands use `program arg...`.

### Child output

Commands inherit stdout and stderr directly.

### Failures

Fatal command failures exit non-zero immediately and later commands do not run.
In either phase, a command that cannot be spawned reports
`treeboot: failed to run command <label>: <io-error>` to stderr and exits
non-zero unless `allow_failure = true`.

Allowed failures always emit a phase-labelled warning and do not make the
command fail by themselves:

```text
treeboot: warning: command optional lint: npm run lint failed with exit status: 1
treeboot: warning: teardown command Stop services: docker compose down failed with exit status 1
```

Execution-time cwd resolution or boundary failures use the same phase-specific
command-start failure behavior and prevent that command from spawning. Under
`allow_failure = false` the failure is fatal; under `allow_failure = true` it
warns and later commands continue.

### Cross-platform contract

Windows support is part of the design contract. Implementation and tests must
account for platform differences in shell execution, path handling, symlink
creation, and canonical path presentation.

## Operator experience: Output and exit codes

Output should be concise enough for setup logs while still making skipped
targets and destructive choices obvious.

```text
treeboot: copy .env.local -> .env.local
treeboot: skip copy .env.local; target exists
treeboot: symlink .tool-versions -> ../repo/.tool-versions
treeboot: sync shared/config -> .config
treeboot: sync metadata shared/editor/settings.json -> .editor/settings.json
treeboot: sync shared -> shared (4 changed, 1 deleted)
treeboot: run Install packages: npm install
treeboot: teardown run Stop services: docker compose down
treeboot: warning: could not preserve ownership shared/cache: operation not permitted
treeboot: warning: command optional lint: npm run lint failed with exit status: 1
```

Unchanged sync files and directories produce no output. Sync reports creates,
content updates, metadata-only updates, and deletes directly for single concrete
actions and as grouped counts for expanded directory operations. Command child
output is inherited directly.

Metadata-only sync updates use the same source and target display style as
content updates:

```text
treeboot: sync metadata shared/config -> shared/config
treeboot: would sync metadata shared/config -> shared/config
treeboot: warning: could not preserve ownership shared/config: operation not permitted
```

Interactive progress is ephemeral terminal UI, not durable log output. It is
rendered to stderr only for non-verbose copy and sync operations on interactive
terminals. Final summaries, warnings, and command lifecycle lines remain normal
`treeboot:` lines.

Command start lines use `treeboot: run <label>`. Dry-run uses
`treeboot: would run <label>` and does not spawn commands. Fatal command
failures are reported as `treeboot: command <label> failed with <status>`. Fatal
spawn failures are reported as
`treeboot: failed to run command <label>: <io-error>`. Allowed spawn failures
are reported as
`treeboot: warning: command <label> failed to start: <io-error>`.

Teardown command start lines use `treeboot: teardown run <label>`. Teardown
dry-run uses `treeboot: teardown would run <label>`. Teardown allowed-failure
messages identify the teardown phase. Discovery output remains shared:

```text
treeboot: config detected /repo/worktrees/feature/.treeboot.toml
treeboot: teardown run Stop services: docker compose down
treeboot: teardown run Drop database: mise run db:drop
```

No-op output is durable:

```text
treeboot: no config detected
treeboot: no teardown commands configured
```

Manual file operation validation errors should identify the CLI operation,
source, and target involved. They must not report synthetic config file paths,
TOML line numbers, or TOML column numbers for command-line arguments. Config
parse or normalization errors found while loading manual command policy still
report the real config path and TOML location.

| Exit | Meaning                                                                                                             |
| ---- | ------------------------------------------------------------------------------------------------------------------- |
| `0`  | Success, including missing discovered config or no teardown commands.                                               |
| `1`  | Runtime failure, config error, operation or command failure, teardown decline, or missing non-interactive approval. |
| `2`  | CLI usage error.                                                                                                    |

## Distribution: Install and releases

Release assets should be predictable enough for direct GitHub release installers
such as `ubi` and `mise`.

Archive assets:

```text
treeboot-aarch64-apple-darwin.tar.gz
treeboot-x86_64-apple-darwin.tar.gz
treeboot-x86_64-unknown-linux-musl.tar.gz
treeboot-aarch64-unknown-linux-musl.tar.gz
treeboot-x86_64-pc-windows-msvc.zip
treeboot-aarch64-pc-windows-msvc.zip
treeboot-x86_64-android.tar.gz
treeboot-aarch64-android.tar.gz
```

Raw executable assets:

```text
treeboot-aarch64-apple-darwin
treeboot-x86_64-apple-darwin
treeboot-x86_64-unknown-linux-musl
treeboot-aarch64-unknown-linux-musl
treeboot-x86_64-pc-windows-msvc.exe
treeboot-aarch64-pc-windows-msvc.exe
treeboot-x86_64-android
treeboot-aarch64-android
```

Release metadata assets:

```text
treeboot-checksums.txt
config.schema.json
treeboot-sbom.spdx.json
```

- **Archive contents**: `treeboot`, `README.md`, and `LICENSE`. Android asset
  labels omit the Rust target triple's `linux` segment so desktop Linux
  installers do not classify Android archives as generic Linux assets.
- **Raw executable assets**: Publish the platform executable itself as a
  separate asset so installers can download, chmod when needed, and run without
  unpacking an archive.
- **Config schema**: Publish the config JSON Schema as `config.schema.json`. It
  should match the checked-in `schemas/treeboot.schema.json` generated from the
  Rust schema model.
- **Checksums**: Publish a checksum manifest that covers every release asset
  uploaded to the GitHub Release, including archives, raw executables, the
  config schema, and SBOMs.
- **GPG signatures**: Planned distribution hardening should publish one detached
  GPG signature for `treeboot-checksums.txt`. The checksum manifest is the
  signed statement for the other release assets.
- **SBOM**: Publish a machine-readable SPDX JSON software bill of materials for
  each release.
- **Attestations**: Publish provenance attestations from GitHub Actions release
  automation. Consumers should be able to verify release assets with
  `gh attestation verify`.
- **Apple signing**: Planned distribution hardening should sign macOS CLI
  binaries with Apple Developer ID and notarize them through Apple's developer
  tooling before publication.
- **Release targets**: macOS Apple Silicon, macOS Intel, Linux x86_64 musl,
  Linux ARM64 musl, Windows x86_64/ARM64 MSVC, and Android x86_64/ARM64.
- **Crates.io packages**: Publish `treeboot-core` before `treeboot`. The
  `treeboot` package depends on the matching registry version of `treeboot-core`
  when published, while local development continues to use the workspace path.
- **Target source**: The expanded target list uses triples available from
  `rustc --print target-list`. Release automation should only publish targets
  that build and pass the configured release smoke test on the selected runner.
- **Release flow**: Release PR automation updates version files and
  `CHANGELOG.md`, creates a `vX.Y.Z` tag, and leaves a draft GitHub Release.
  Tag-triggered release automation builds assets, reuses that draft when
  present, falls back to the matching changelog section for release notes when
  needed, uploads assets, and publishes the release only after uploads complete.

## Verification: Testing strategy

The test suite should prove the behavior that users will rely on: discovery,
idempotency, compatibility env vars, and real Git worktree behavior.

### Unit tests

- Config parsing.
- Duplicate file operation target detection.
- Outside-worktree target validation.
- String and object file parsing.
- Sync comparison and explicit delete behavior.
- String and object command parsing.
- Compact and verbose teardown command parsing and declaration order.
- Whole-document normalization failure before phase planning.
- Independent bootstrap and teardown semantic validation.
- Discovery order.
- Environment variable construction.
- Conflict mode behavior.
- Relative symlink calculation.
- Manual source-to-target normalization.

### Integration tests

- Create a temporary Git repository.
- Create a linked worktree.
- Run treeboot from the linked worktree.
- Prepare and execute teardown for the current and an explicitly selected linked
  worktree.
- Run manual copy, symlink, and sync operations.
- Verify files, symlinks, bootstrap and teardown commands, and env vars.
- Revalidate command cwd immediately before both bootstrap and teardown spawn.

### CLI tests

- `treeboot` equals `treeboot run`.
- `teardown` requires approval, rejects the root checkout, and never removes a
  worktree.
- `teardown --dry-run` neither prompts nor spawns.
- missing discovered config and empty teardown commands are no-op successes.
- non-terminal teardown requires `--yes`; refusal exits `1`.
- `status` reports discovery paths without execution.
- `init` creates config by default.
- `init --config` creates config.
- `copy`, `symlink`, and `sync` require sources.
- Manual `--target` handles one and many sources.
- Manual operation source completion reads from root path.
- `completions` emits scripts for supported shells.
- Conflict flags behave as specified.

This Markdown document is the project specification for treeboot.

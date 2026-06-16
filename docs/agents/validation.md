# Validation Guide

Use this guide to pick the smallest useful feedback loop for a change.

## Tiers

### Targeted

Use while iterating on a narrow change:

```sh
mise run test:core
mise run test:cli
```

Use `test:core` for library behavior and `test:cli` for user-visible command
behavior. Running `mise run test` executes both through mise dependencies.

### Check

Use before handoff for most code changes:

```sh
mise run check
```

This runs formatting, clippy, and tests.

### Verify

Use for broad, CI-facing, release-facing, or harness changes:

```sh
mise run verify
```

This runs the local CI task set plus coverage. Coverage is not a required merge
gate; it is a sensor for finding untested behavior.

## CI Mapping

GitHub Actions runs these mise tasks:

- `mise run actions:lint`
- `mise run fmt`
- `mise run lint`
- `mise run msrv`
- `mise run test:core`
- `mise run test:cli`

The local `mise run ci` task mirrors those checks.

## Coverage

For quick coverage feedback:

```sh
mise run coverage
```

The current suite is intentionally strongest around milestone 1 behavior:
script discovery/execution, config detection, init output creation, environment
propagation, and output formatting.

Useful follow-up coverage areas:

- TOML config parsing and normalization once implemented
- file-operation validation before side effects
- command runtime and async batching
- sync conflict and delete-extra behavior

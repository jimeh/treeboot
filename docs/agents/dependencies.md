# Dependency Intake

Use dependencies when they simplify real behavior or reduce risk. Avoid adding
dependencies for small wrappers around the standard library.

## Current Choices

- `clap` belongs in the `treeboot` CLI crate for argument parsing.
- `thiserror` belongs in `treeboot-core` for public typed errors.
- `assert_cmd`, `predicates`, and `tempfile` support CLI integration tests.
- `cargo-llvm-cov` is a mise-managed development tool, not a Cargo dependency.

## Guidelines

- Keep `treeboot-core` free of CLI-only dependencies such as `clap`.
- Keep `anyhow` out of `treeboot-core`; public library errors should stay typed.
- Prefer `std::path::Path` and `PathBuf` unless path handling needs a stronger
  abstraction.
- Prefer the Git CLI over a Git library unless the spec requires behavior that
  the CLI cannot provide reliably.
- Add a dependency only when the reason is clear in the surrounding change.

## Review Checklist

Before adding a new dependency:

```sh
cargo tree -p treeboot
cargo tree -p treeboot-core
mise run check
```

Check whether the dependency:

- belongs in the CLI crate, core crate, dev-dependencies, or mise tools
- affects MSRV
- pulls in surprising transitive dependencies
- duplicates an existing dependency or standard-library capability
- changes public API commitments for `treeboot-core`

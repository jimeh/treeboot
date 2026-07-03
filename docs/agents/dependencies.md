# Dependency Intake

Use dependencies when they simplify real behavior or reduce risk. Avoid adding
dependencies for small wrappers around the standard library.

## Current Choices

- `clap` belongs in the `treeboot` CLI crate for argument parsing.
- `indicatif` belongs in the `treeboot` CLI crate for interactive file operation
  progress. Keep progress rendering out of `treeboot-core`; core should emit
  structured output events instead.
- `console` belongs in the `treeboot` CLI crate when progress rendering needs
  terminal width or Unicode-aware text measurement. Keep terminal-specific
  formatting helpers out of `treeboot-core`.
- `thiserror` belongs in `treeboot-core` for public typed errors.
- `dunce` belongs in `treeboot-core` for path canonicalization. It wraps
  `std::fs::canonicalize` but strips Windows `\\?\` verbatim prefixes when the
  path can be represented safely in conventional form. Use the repo-local path
  helpers rather than calling it directly so output, environment variables, and
  boundary checks compare paths consistently across platforms.
- `serde` and `toml` belong in `treeboot-core` for declarative config parsing
  and normalized model serialization.
- `ignore` belongs in `treeboot-core` for operation-local gitignore-style path
  matching. Use `ignore::gitignore::GitignoreBuilder` with explicit `add_line`
  patterns only; do not load ambient `.gitignore`, `.ignore`, `.rgignore`, Git
  exclude, or global ignore files for treeboot file operations.
- `globset` belongs in `treeboot-core` for matching declarative and manual file
  operation source glob patterns. Build matchers with `literal_separator(true)`
  and `backslash_escape(false)` so source patterns stay consistent with
  gitignore-style ignore rules and Windows path separators; keep source
  enumeration and boundary checks in treeboot's own validation layer.
- `serde_json` belongs in the `treeboot` CLI crate for
  `treeboot config --format json` and other JSON report rendering.
- `yaml_serde` belongs in the `treeboot` CLI crate for YAML report rendering. It
  is the maintained YAML organization fork of the deprecated `serde_yaml` crate.
- `schemars` and `serde_json` are dev-dependencies in `treeboot-core` for the
  JSON Schema generator example.
- `assert_cmd`, `predicates`, and `tempfile` support CLI integration tests.
- `markdown` belongs in `tools/release-helper` so release-note extraction can
  identify changelog sections structurally while preserving source Markdown.
- `zip` belongs in `tools/release-helper` so Windows release archives do not
  depend on Python or platform-specific zip tools in CI.
- `cargo-llvm-cov` is a task-scoped mise development tool, not a Cargo
  dependency.
- Mise-managed tools use a 7-day `minimum_release_age` cooldown and checked-in
  `mise.lock` to avoid adopting freshly published binaries by default.

## Guidelines

- Keep `treeboot-core` free of CLI-only dependencies such as `clap`.
- Keep `anyhow` out of `treeboot-core`; public library errors should stay typed.
- Prefer `std::path::Path` and `PathBuf` unless path handling needs a stronger
  abstraction.
- Prefer the Git CLI over a Git library unless the spec requires behavior that
  the CLI cannot provide reliably.
- Add a dependency only when the reason is clear in the surrounding change.
- For urgent security or CI-maintenance updates that must bypass the mise
  cooldown, use the narrowest one-off override and call it out in the PR.

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

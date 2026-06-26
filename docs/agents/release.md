# Release Harness

Release automation is split between release-please and tag-triggered asset
publication. Use this guide when maintaining the release milestone from the
spec.

## Release Contract

The spec expects:

- archive assets for each supported target
- raw executable assets for direct installers
- `config.schema.json`
- `treeboot-checksums.txt`
- SPDX SBOM
- GitHub artifact attestations for release provenance

GPG checksum signing and macOS signing/notarization are planned hardening work,
not part of the first release automation pass.

Supported release targets are macOS Apple Silicon, macOS Intel, Linux x86_64
musl, Linux ARM64 musl, Windows x86_64/ARM64 MSVC, and Android x86_64/ARM64.

Release-please creates release PRs, updates `CHANGELOG.md`, bumps Cargo
versions, creates `vX.Y.Z` tags, and leaves draft GitHub Releases. It must run
with a GitHub App token so tag pushes trigger the release workflow.
While `treeboot` is pre-1.0, release-please treats breaking changes as minor
bumps instead of major bumps.

The tag-triggered release workflow should reuse the draft GitHub Release for
the pushed tag. If no draft exists, it should extract the matching changelog
section as release notes, create a draft, upload all assets, publish the
crates.io packages, and publish the GitHub Release only after uploads and crate
publication complete.

The manual release workflow path should generate the same build assets but
default to workflow artifacts only. It must not publish a GitHub Release or
crates.io packages. Manual runs derive their test artifact version from the
checked-out Git state; do not add a manual version input.

Crates.io publishing uses two packages: publish `treeboot-core` first, then
publish `treeboot` after the registry index can resolve the matching
`treeboot-core` version. The CLI package must keep its `treeboot-core`
dependency as both a local `path` and the matching registry `version` so local
workspace development and published dependency resolution both work.
Publishing is authenticated with crates.io Trusted Publishing, bound to the
GitHub Actions `release` environment in `.github/workflows/release.yml`.
Reruns should check crates.io first and skip any package version that is already
published.

Release workflow scripts in `scripts/` are thin wrappers around the Rust
`treeboot-release-helper` workspace package. Keep release version derivation,
asset packaging, and changelog release-note extraction in that helper so the
logic is linted and tested with the rest of the workspace. CI executes it via
the wrappers, which call `cargo run --quiet -p treeboot-release-helper --locked
-- <subcommand>`.

## Future Tasks

Before the first real release, add or document commands for:

- generating shell completion scripts for bash, zsh, fish, powershell, and
  elvish from the built binary
- signing checksums
- signing/notarizing macOS binaries

## Validation Expectations

Release work should run:

```sh
mise run verify
```

Release-specific automation should also have at least one local dry-run or smoke
command that does not publish anything:

```sh
mise run release:check
mise run release:package:local
cargo publish --dry-run -p treeboot-core --locked
cargo publish --dry-run -p treeboot --locked
```

Use `release:check` as the default release-maintenance gate. It packages the
current host artifact and smoke-checks completion generation for every supported
shell.

Before publishing a new version, dry-run both crates in publish order. If
`treeboot-core` has not been published for that version yet, the `treeboot`
dry-run may only fully verify after the matching core version reaches the
registry index; use `cargo package -p treeboot --list` to inspect the CLI
package contents before then.

Before publishing, review install notes for shell completion paths and run
completion generation for every supported shell.

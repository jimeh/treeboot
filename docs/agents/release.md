# Release Harness

Release automation is split between release-please and tag-triggered asset
publication. Use this guide when maintaining the release milestone from the
spec.

## Release Contract

The spec expects:

- archive assets for each supported target
- raw executable assets for direct installers
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

The tag-triggered release workflow should reuse the draft GitHub Release for
the pushed tag. If no draft exists, it should extract the matching changelog
section as release notes, create a draft, upload all assets, and publish only
after uploads complete.

The manual release workflow path should generate the same build assets but
default to workflow artifacts only. It should not publish a GitHub Release
unless explicitly requested.

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
mise run release:package:local
```

Before publishing, review install notes for shell completion paths and run
completion generation for every supported shell.

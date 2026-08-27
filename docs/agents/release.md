# Release Harness

Release automation is split between release-please and tag-triggered asset
publication. This guide owns the official Treeboot distribution and publication
contract. Those implementation-specific mechanics do not belong in the
language-agnostic CLI specification.

## Release Contract

Release assets use these names so direct GitHub release installers such as `ubi`
and `mise` can select them predictably.

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

Unix archives contain `treeboot`, `README.md`, and `LICENSE`. Windows archives
contain `treeboot.exe`, `README.md`, and `LICENSE`. Publish the raw platform
executable separately so installers can download it, make it executable when
needed, and run it without unpacking an archive. Publish `config.schema.json`
from the canonical checked-in
`crates/treeboot-spec/assets/treeboot.schema.json`.

The checksum manifest covers every other asset uploaded to the GitHub Release,
including archives, raw executables, the config schema, and SBOMs. Publish one
machine-readable SPDX JSON SBOM for the release and provenance attestations from
GitHub Actions. Consumers can verify release assets with
`gh attestation verify`.

The supported release targets are:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`
- `x86_64-pc-windows-msvc`
- `aarch64-pc-windows-msvc`
- `x86_64-linux-android`
- `aarch64-linux-android`

Android asset labels omit the Rust target triple's `linux` segment so desktop
Linux installers do not classify Android archives as generic Linux assets. The
target list comes from `rustc --print target-list`. Release automation publishes
only targets that build and pass their configured release smoke test on the
selected runner.

GPG checksum signing and macOS signing/notarization are planned hardening work,
not part of the current release automation. The planned signing flow publishes
one detached GPG signature for `treeboot-checksums.txt`, making the checksum
manifest the signed statement for other assets. The planned macOS flow signs CLI
binaries with Apple Developer ID and notarizes them before publication.

Release-please creates release PRs, updates `CHANGELOG.md`, bumps Cargo
versions, creates `vX.Y.Z` tags, and leaves draft GitHub Releases. It must run
with a GitHub App token so tag pushes trigger the release workflow. While
`treeboot` is pre-1.0, release-please treats breaking changes as minor bumps
instead of major bumps.

The tag-triggered release workflow should reuse the draft GitHub Release for the
pushed tag. If no draft exists, it should extract the matching changelog section
as release notes, create a draft, upload all assets, publish the crates.io and
npm packages, and publish the GitHub Release only after uploads and both
registries complete.

The manual release workflow path should generate the same build and npm assets
but default to workflow artifacts only. It must not publish a GitHub Release or
registry packages. Manual runs derive their test artifact version from the
checked-out Git state; do not add a manual version input.

Crates.io publishing uses three packages: publish `treeboot-spec` first,
`treeboot-core` second, then publish `treeboot` after the registry index can
resolve the matching `treeboot-core` version. The CLI package must keep its
`treeboot-core` dependency as both a local `path` and the matching registry
`version` so local workspace development and published dependency resolution
both work. All three crate publishers use crates.io Trusted Publishing, bound to
the GitHub Actions `release` environment in `.github/workflows/release.yml`.
Reruns should check crates.io first and skip any package version that is already
published.

Before the first tag that includes `treeboot-spec`, reserve the crate name with
a manual token-authenticated initial publish. After that publish succeeds,
configure its crates.io Trusted Publisher for the GitHub Actions `release`
environment in `.github/workflows/release.yml`. Do not create the tag until both
steps are complete. The release workflow intentionally fails closed if
`treeboot-spec` cannot publish before `treeboot-core` and `treeboot`.

npm publishing uses the unscoped `treeboot` facade and six platform packages
under `@treeboot-rs`. The TypeScript packager consumes the raw desktop
executables assembled in `dist/`, creates seven tarballs under `npm-dist/`, and
writes `manifest.json` with exact SHA-512 integrity values. The npm artifact is
separate from GitHub Release assets and their checksum manifest.

Publish the six platform packages first, wait for each exact version and
integrity to appear, then publish `treeboot` last. On reruns, skip an existing
version only when npm's `dist.integrity` matches the staged tarball. A mismatch
must fail the release. npm uses Trusted Publishing from the same `release`
environment and `.github/workflows/release.yml`, with Node.js 24, npm 11.15 or
newer, and no `NPM_TOKEN`. GitHub Actions must retain `id-token: write`.

All npm source manifests stay `private` with version `0.0.0-development`.
Publish only tarballs produced and checked by `npm/scripts/package-release.ts`
and `npm/scripts/verify-package.ts`. Never run `npm publish` from the repository
root or a source workspace.

Before the first functional release, a maintainer must publish the six inert
`0.0.0` platform placeholders, configure all seven package trusted-publisher
relationships, and verify them with `npm trust list`. The exact one-time
commands are in [npm-distribution-plan.md](npm-distribution-plan.md). After the
first successful OIDC release, set every package to require 2FA and disallow
token publication.

Release workflow scripts in `scripts/` are thin wrappers around the Rust
`treeboot-release-helper` workspace package. Keep release version derivation,
asset packaging, and changelog release-note extraction in that helper so the
logic is linted and tested with the rest of the workspace. CI executes it via
the wrappers, which call
`cargo run --quiet -p treeboot-release-helper --locked -- <subcommand>`.

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
cargo publish --dry-run -p treeboot-spec --locked
mise run npm:pack -- "$(mise run release:version)" dist npm-dist
mise run npm:pack:check npm-dist
cargo publish --dry-run -p treeboot-core --locked
cargo publish --dry-run -p treeboot --locked
```

Use `release:check` as the default release-maintenance gate. It packages the
current host artifact and smoke-checks completion generation for every supported
shell.

Before publishing a new version, dry-run all three crates in publish order. If
`treeboot-core` has not been published for that version yet, the `treeboot`
dry-run may only fully verify after the matching core version reaches the
registry index; use `cargo package -p treeboot --list` to inspect the CLI
package contents before then.

Before publishing, review install notes for shell completion paths and run
completion generation for every supported shell.

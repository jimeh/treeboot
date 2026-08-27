# npm Distribution Plan

Status: ready for implementation; maintainer registry bootstrap is required
before the first tagged npm release.

## Objective

Publish Treeboot as an npm package that Node.js tools and Electron applications
can declare as a normal dependency, without maintaining their own GitHub Release
downloader.

The npm distribution is an integration surface, not the preferred replacement
for installing Treeboot directly in a project. The README should continue to
recommend mise for that use case. `npx treeboot` and `bunx treeboot` are useful
conveniences once the package exists, but they are secondary to:

```sh
bun add treeboot
```

```ts
import { getBinaryPath, spawnTreeboot } from "treeboot";
```

The implementation target is one pull request that adds the package sources,
tests, documentation, CI coverage, release packaging, and npm publication to the
existing Treeboot release lifecycle.

## Settled Decisions

- Publish the public facade as the unscoped `treeboot` package.
- Publish platform binaries under the `@treeboot-rs` npm organization.
- Support Node.js 22 and newer.
- Use TypeScript wherever practical for the facade, CLI shim, release packager,
  and npm-specific tests.
- Use Bun for local package management, TypeScript execution, builds, and tests.
  Keep mise as the repository task surface.
- Install TypeScript 7, Oxfmt, Oxlint, and the Oxlint type-aware plugin as Bun
  development dependencies. Remove the separately mise-managed Oxfmt tool.
- Publish the existing Rust executables; do not introduce N-API bindings, WASM,
  or a second implementation of Treeboot's Rust library API.
- Use platform-specific optional dependencies. Do not download executables in an
  install script.
- Publish through npm Trusted Publishing from the existing GitHub Actions
  release workflow. Do not add an `NPM_TOKEN` secret.
- Keep every npm package on the same version as the Treeboot Git tag and Cargo
  workspace release.
- Cover the six desktop platforms already represented by the CI runner matrix.
  Android npm packages are a future extension, not part of this pull request.

## Package Architecture

The release contains one facade package and six binary packages:

| Package                     | npm platform        | Rust release executable                |
| --------------------------- | ------------------- | -------------------------------------- |
| `treeboot`                  | portable JavaScript | Selects one optional dependency        |
| `@treeboot-rs/darwin-arm64` | `darwin` / `arm64`  | `treeboot-aarch64-apple-darwin`        |
| `@treeboot-rs/darwin-x64`   | `darwin` / `x64`    | `treeboot-x86_64-apple-darwin`         |
| `@treeboot-rs/linux-arm64`  | `linux` / `arm64`   | `treeboot-aarch64-unknown-linux-musl`  |
| `@treeboot-rs/linux-x64`    | `linux` / `x64`     | `treeboot-x86_64-unknown-linux-musl`   |
| `@treeboot-rs/win32-arm64`  | `win32` / `arm64`   | `treeboot-aarch64-pc-windows-msvc.exe` |
| `@treeboot-rs/win32-x64`    | `win32` / `x64`     | `treeboot-x86_64-pc-windows-msvc.exe`  |

The facade declares all six binary packages in `optionalDependencies`, pinned to
the exact facade version. Each binary package declares its `os` and `cpu`
constraints so npm-compatible clients install only the matching package. The
Linux packages intentionally omit npm's `libc` restriction because Treeboot's
existing static musl executables are intended to run across Linux libc
environments.

Each platform package contains only:

- `package.json`;
- the executable under `bin/`;
- a short package-specific `README.md`;
- the Treeboot license.

Platform packages have no JavaScript, dependencies, lifecycle scripts, or
network behavior. All seven packages declare their repository and release
metadata and use `publishConfig.access = "public"`.

This layout follows npm's documented
[`os`, `cpu`, and `optionalDependencies` behavior](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/).
It keeps normal installs small, supports offline and reproducible installs, and
lets Electron packagers see the executable as an ordinary dependency.

## Facade Contract

The first public JavaScript API should stay deliberately small:

- `getBinaryPath()` returns the absolute path to the executable selected for the
  current `process.platform` and `process.arch`.
- `spawnTreeboot(args?, options?)` starts Treeboot without invoking a shell and
  returns the Node.js child process.
- `spawnTreebootSync(args?, options?)` runs the same operation synchronously.
- Typed exported errors distinguish an unsupported platform from a compatible
  optional package that is missing from the installation.

The spawn option types should derive from `node:child_process`, but omit the
command, argument vector, and `shell` fields owned by the wrapper. User
arguments must always be passed as an array rather than interpolated into a
command string.

The package also exposes a `treeboot` executable shim. It resolves the same
binary, inherits standard input/output/error, forwards ordinary termination
signals, and exits with the child process's exit status. This supports:

```sh
npx treeboot --version
bunx treeboot --version
```

The package publishes:

- Node-compatible ESM;
- Node-compatible CommonJS;
- TypeScript declarations;
- an executable Node.js CLI entrypoint.

Published code may use Node.js built-ins but not Bun-only APIs. Bun is the
development runtime and build tool; Node.js is the consumer runtime contract.
The facade declares `engines.node = ">=22"`.

### Resolution failures

Errors must explain the detected platform and architecture, the expected npm
package, and likely causes. A missing binary dependency should specifically
mention disabled optional dependencies and bundlers or Electron packagers that
pruned the package. It must not silently fall back to downloading a release or
to an unrelated executable on `PATH`.

## Electron Contract

Treeboot belongs in Electron's main process, a utility process with Node.js
access, or an application backend. It is not a renderer API.

Electron applications must keep the selected platform package in the packaged
application and leave its executable outside `app.asar`. Electron only supports
`execFile` directly for binaries inside ASAR; `spawn`, which this facade uses,
requires a real filesystem path. The package documentation must therefore show
how to mark `node_modules/@treeboot-rs/**/bin/**` as unpacked or copy it to the
packager's external resources. See Electron's
[ASAR execution limitations](https://www.electronjs.org/docs/latest/tutorial/asar-archives#executing-binaries-inside-asar-archive).

As a defensive convenience, `getBinaryPath()` should detect a resolved path
inside an `.asar` archive and return the matching `.asar.unpacked` path when
that real file exists. It must otherwise return the original path so an invalid
packaging configuration produces a direct, diagnosable failure.

Cross-architecture and universal Electron builds cannot rely on optional
dependency selection performed on the build host. Their build configuration must
explicitly install and retain every target package they ship. Document this
constraint and verify package selection independently from the build machine's
architecture.

## Source Layout and Development Tooling

Add a private Bun workspace at the repository root:

```text
package.json
bun.lock
tsconfig.json
.oxfmtrc.json
oxlint.json
npm/
  treeboot/
    package.json
    README.md
    src/
      cli.ts
      errors.ts
      index.ts
      platform.ts
  platforms/
    darwin-arm64/
    darwin-x64/
    linux-arm64/
    linux-x64/
    win32-arm64/
    win32-x64/
  scripts/
    package-release.ts
    verify-package.ts
  tests/
```

The root workspace is never published. Source manifests for the seven public
packages also remain safe to work with directly by using `private: true`, the
sentinel version `0.0.0-development`, and workspace dependency references where
needed.

The TypeScript release packager creates isolated publishable directories. It
removes `private`, stamps the tag-derived version, rewrites all facade optional
dependencies to that exact version, copies the matching release executable,
builds the facade, and produces `.tgz` files under ignored `npm-dist/`.
Publishing must only accept those generated tarballs; neither local commands nor
CI should run `npm publish` from a source workspace directory.

This staging boundary avoids scattering release-version literals through the
source tree and keeps release-please's existing single root Rust release unit.
It also makes the exact bytes tested during assembly the bytes later published.

### Tool ownership

- Add Bun to `mise.toml` and lock its resolved tool version in `mise.lock`.
- Keep mise tasks as the discoverable interface, delegating npm work to Bun
  scripts.
- Install `typescript` 7, `@types/node` 22, `oxfmt`, `oxlint`, and
  `oxlint-tsgolint` in root `devDependencies`, with exact resolutions in
  `bun.lock`.
- Remove Oxfmt from mise once the Bun-backed formatting tasks cover the existing
  Markdown paths as well as TypeScript and JSON.
- Extend `mise run setup`, `format`, `lint`, `check`, and `verify` rather than
  creating a parallel validation surface.
- Add focused tasks such as `npm:build`, `npm:test`, `npm:pack`, and
  `npm:pack:check` for iteration and release dry runs.
- Add `node_modules/`, `npm-dist/`, and generated workspace build directories to
  `.gitignore`.
- Add a monthly Dependabot Bun ecosystem entry with the repository's existing
  cooldown and grouping conventions. GitHub documents `bun.lock` support in its
  [Dependabot ecosystem matrix](https://docs.github.com/en/code-security/reference/supply-chain-security/supported-ecosystems-and-repositories).

## Release Integration

Extend `.github/workflows/release.yml`; do not create a second release
lifecycle.

### Assembly

1. Keep the existing target matrix responsible for compiling and smoke-testing
   Rust executables.
2. Keep the Rust release helper responsible for the canonical raw executable
   assets and archives.
3. In the assembly job, install the locked Bun toolchain and dependencies.
4. Pass the tag-derived version and the collected raw executables to the
   TypeScript npm release packager.
5. Generate seven deterministic npm tarballs and a machine-readable manifest
   containing package name, version, filename, size, and SHA-512 integrity.
6. Verify the tarball contents before uploading them as one private workflow
   artifact for the publish job.

The manual workflow path must build and verify the same tarballs but never
publish them. It provides a safe end-to-end rehearsal and downloadable evidence
for review. npm tarballs do not need to become GitHub Release assets or enter
the existing release checksum file; the registry and its provenance statement
are their distribution surface.

### Publication

The existing tag-only `publish` job already uses the `release` GitHub
environment and requests `id-token: write`. Extend it as follows:

1. Download the exact npm tarball artifact produced by assembly.
2. Set up Node.js 24 on a GitHub-hosted runner with npm caching disabled.
3. Ensure npm is at least 11.15.0. Bun remains the build/test tool; npm CLI is
   used here only because it is the Trusted Publishing client.
4. Publish all six platform packages first.
5. Wait until each exact package version is visible from the npm registry.
6. Publish the `treeboot` facade last. The facade is the commit point: users
   cannot receive a version whose optional packages are still unavailable.
7. Continue to publish the two crates.io packages and GitHub Release assets in
   the same job dependency graph.
8. Mark the GitHub Release non-draft only after npm, crates.io, and asset
   publication all succeed.

npm Trusted Publishing requires a GitHub-hosted runner, `id-token: write`,
Node.js 22.14 or newer, and npm 11.5.1 or newer. It removes the long-lived npm
token and automatically attaches provenance for public packages published from a
public GitHub repository. See npm's
[Trusted Publishing guide](https://docs.npmjs.com/trusted-publishers/).

### Safe reruns

Package versions are immutable, so tag-workflow reruns must be idempotent. For
each tarball:

- query the exact `package@version` before publishing;
- publish when it does not exist;
- when it does exist, compare the registry's `dist.integrity` with the local
  tarball's integrity and skip only on an exact match;
- fail on a mismatch rather than treating any existing version as success.

This is stricter than the current crates.io existence check because a
multi-package npm release can stop halfway through and then be rerun.

## Maintainer Publishing Setup

These are one-time npm registry actions. They are not repository secrets and
cannot be completed by a pull request alone.

Current registry state as of 2026-08-26:

- `treeboot@0.0.0` exists and holds the unscoped name. It is deprecated with a
  message directing users to mise until the functional package ships.
- The six proposed `@treeboot-rs/*` package names returned `404 Not Found` and
  still need to be created.
- npm Trusted Publishing configuration is per package, so all seven packages
  need their own relationship.

The GitHub-side values are already settled:

| Field                       | Value         |
| --------------------------- | ------------- |
| GitHub organization or user | `jimeh`       |
| Repository                  | `treeboot`    |
| Workflow filename           | `release.yml` |
| GitHub environment          | `release`     |
| Allowed action              | `npm publish` |

The repository already has the `release` environment, restricted to `v*` tags,
and the publish job already requests an OIDC token. No `NPM_TOKEN` GitHub secret
should be added.

### 1. Check local npm administration prerequisites

The `npm trust` command requires npm 11.15.0 or newer, an npm account with 2FA,
write access to each package, and an existing package. These requirements and
the command flags are documented in the
[`npm trust` reference](https://docs.npmjs.com/cli/v11/commands/npm-trust/).

```sh
npm --version
npm whoami
```

If necessary, update the administrative CLI before continuing:

```sh
npm install --global 'npm@^11.15.0'
```

### 2. Configure the existing facade package now

```sh
npm trust github treeboot \
  --repository jimeh/treeboot \
  --file release.yml \
  --environment release \
  --allow-publish
```

Complete the npm 2FA/browser prompt, then verify the recorded relationship:

```sh
npm trust list treeboot
```

### 3. Create the six platform packages

npm will not allow a trusted publisher to be configured before a package exists.
During implementation, generate and inspect minimal `0.0.0` placeholder tarballs
for the six platform packages, then publish them manually with the authenticated
maintainer account. Each placeholder must be public, contain no executable or
lifecycle script, and carry the same temporary deprecation notice as
`treeboot@0.0.0`.

Do this from the reviewed tarballs produced for that checkpoint, not by running
`npm publish` from the repository root. This manual bootstrap is the only
token-authenticated publication expected in the final design.

### 4. Configure all platform trusted publishers

After all six placeholders exist, run:

```sh
packages=(
  '@treeboot-rs/darwin-arm64'
  '@treeboot-rs/darwin-x64'
  '@treeboot-rs/linux-arm64'
  '@treeboot-rs/linux-x64'
  '@treeboot-rs/win32-arm64'
  '@treeboot-rs/win32-x64'
)

for package_name in "${packages[@]}"; do
  npm trust github "$package_name" \
    --repository jimeh/treeboot \
    --file release.yml \
    --environment release \
    --allow-publish \
    --yes
  sleep 2
done
```

npm may offer a short 2FA skip window after the first confirmation. The delay
matches npm's bulk-configuration advice and reduces rate-limit risk.

Verify each relationship:

```sh
npm trust list treeboot
npm trust list '@treeboot-rs/darwin-arm64'
npm trust list '@treeboot-rs/darwin-x64'
npm trust list '@treeboot-rs/linux-arm64'
npm trust list '@treeboot-rs/linux-x64'
npm trust list '@treeboot-rs/win32-arm64'
npm trust list '@treeboot-rs/win32-x64'
```

The equivalent website path is each package's **Settings > Trusted publishing**
page. Enter only `release.yml`, not `.github/workflows/release.yml`, and
preserve the exact case of every field.

### 5. Harden access after the first OIDC release

Do this only after a real tagged release proves Trusted Publishing works for all
seven packages:

1. Open each package's **Settings > Publishing access** page.
2. Select **Require two-factor authentication and disallow tokens**.
3. Revoke any npm automation token that is no longer needed.

The token restriction does not block the configured OIDC publisher. If the
repository owner, repository name, workflow filename, or GitHub environment
changes, update all seven npm relationships before the next release. The
crates.io trusted publishers are also tied to the current workflow filename and
environment.

## Implementation Sequence

### 1. Freeze the contract and workspace shape

- Add the npm distribution behavior to `docs/SPEC.md` and bump the spec from
  2.4.0 to 2.5.0.
- Keep the README's referenced spec version in sync.
- Add the private Bun workspace, locked toolchain, lint/format/typecheck config,
  ignored outputs, and mise tasks.
- Update dependency automation for `bun.lock`.

Exit evidence: Bun installs from the lockfile and the existing Rust and Markdown
tasks still work through mise.

### 2. Implement platform resolution and the Node API

- Add the platform table and typed resolution errors.
- Implement ESM and CommonJS exports, declarations, and the CLI shim.
- Add ASAR-unpacked resolution without importing Electron.
- Document Node.js and Electron usage at the package level.

Exit evidence: Node.js 22 loads both module formats, resolver tests cover every
mapping and error, and the CLI wrapper preserves exit behavior.

### 3. Implement safe package staging

- Add source manifests for the facade and six platform packages.
- Build a TypeScript packager that consumes existing raw release executables.
- Stamp one validated semver into every staged manifest.
- Generate and verify all seven tarballs plus the integrity manifest.
- Add a separate placeholder mode for the one-time platform-name bootstrap.

Exit evidence: strict tarball allowlists, exact cross-package versions, correct
`os`/`cpu` metadata, and executable permissions on Unix.

### 4. Add cross-platform installation coverage

- Extend CI to exercise the packed facade on all six existing desktop runner
  architectures.
- Install the packed package with both npm and Bun where practical.
- Run the facade API and `treeboot --version` against the actual Rust
  executable.
- Verify a deliberately omitted optional dependency produces the documented
  typed error.

Exit evidence: the existing six-runner matrix proves package selection and
execution on every advertised platform.

### 5. Integrate release publication

- Produce npm tarballs during release assembly and manual dispatch.
- Publish platform packages first and the facade last through OIDC.
- Add registry visibility waits and integrity-aware rerun handling.
- Keep the final GitHub Release publication behind every registry and asset
  gate.
- Update `docs/ARCHITECTURE.md`, `docs/agents/release.md`, the roadmap, README,
  and agent harness notes.

Exit evidence: a manual release workflow produces inspectable, verified tarballs
without publishing. Repository-local release checks and the full verification
suite pass on the intended PR head.

### 6. Bootstrap registry trust and finish the PR

- Publish and deprecate the six reviewed `0.0.0` platform placeholders.
- Complete the seven trusted-publisher relationships using the maintainer
  runbook above.
- Verify the PR diff against the frozen contract and release invariants.
- Run the repository's required two independent reviews for this public contract
  and release-path change.
- Run `mise run verify` on the final reviewed head and make the PR ready only
  after the required CI checks pass.

Exit evidence: seven package names are owned, seven trust relationships point to
the exact release workflow, and the PR is ready under the repository's normal
final-review policy. The first real tagged release remains the only end-to-end
proof of the npm OIDC exchange.

## Test Strategy

### TypeScript and facade tests

- Type-check with TypeScript 7 using `tsc --noEmit`.
- Run Oxfmt checks and type-aware Oxlint checks.
- Unit-test every supported and unsupported platform/architecture mapping.
- Test missing optional dependencies, missing executable files, and actionable
  error text.
- Test ordinary paths, ASAR paths with an unpacked peer, and ASAR paths without
  one.
- Test async and sync wrappers with arguments containing spaces and shell
  metacharacters to prove no shell interpolation occurs.
- Test CLI exit codes, signals, and standard-stream inheritance.

### Package tests

- Build all seven tarballs with fixture executables on one host.
- Inspect strict file allowlists and reject source maps, tests, configuration,
  credentials, or workspace-only files.
- Assert names, versions, licenses, repository metadata, `os`, `cpu`, `engines`,
  exports, bin entry, and exact optional dependency versions.
- Assert Unix executables and the CLI shim have the required archive modes.
- Install the packed facade under Node.js 22 as both ESM and CommonJS.
- Install with optional dependencies disabled and assert the documented error.
- Confirm package scripts are absent from every platform tarball.

### Cross-platform and release tests

- On macOS ARM64/x64, Linux ARM64/x64, and Windows ARM64/x64, package the
  current host Rust binary, install the facade, resolve it through the API, and
  run `treeboot --version` through the package CLI.
- Exercise installs with both npm and Bun across the matrix where runner support
  is stable.
- Test the publication helper against fake registry responses for absent,
  matching, and mismatched versions.
- Run the manual release workflow and inspect its npm artifact before enabling
  the tag publication path.
- Run `mise run release:check` during implementation and `mise run verify` on
  the final PR head.

A full signed/notarized packaged Electron application is not a required CI test
for this pull request. Cover the ASAR path behavior in fast automated tests and
record one manual packaged Electron smoke test in the PR. Expand this into a
durable Electron fixture only if that smoke test uncovers behavior the unit and
package tests cannot represent.

## Documentation Changes

- `docs/SPEC.md`: npm distribution, platform selection, facade API, CLI shim,
  errors, supported platforms, and Electron constraints.
- `docs/ARCHITECTURE.md`: Bun workspace, facade/platform ownership, staging
  boundary, and release data flow.
- `README.md`: mise remains the primary direct-install recommendation; add npm
  integration, `npx`/`bunx`, and Node/Electron examples.
- `docs/agents/release.md`: seven-package order, OIDC requirements, reruns,
  manual artifacts, and first-publish procedure.
- `docs/agents/roadmap.md`: mark npm distribution scope and deferred targets.
- `AGENTS.md`: record the non-obvious package/release invariants once they are
  implemented.
- Package READMEs: keep the facade documentation user-facing and platform
  package documentation explicit that direct installation is unsupported.

Add an npm version badge only after the first functional package release. Until
then, a badge would advertise the deprecated placeholder.

## Risks and Controls

- **Partial multi-package publication:** publish platforms first, facade last,
  then use integrity-aware idempotent reruns.
- **Platform package pruned by a bundler:** provide a typed error and specific
  Electron/bundler guidance; never perform an implicit network fallback.
- **ASAR execution failure:** document unpacking, prefer an existing
  `.asar.unpacked` peer, and test the path transform.
- **Accidental source-tree publication:** keep source manifests private and
  publish only isolated, verified tarballs.
- **Version drift across ecosystems:** derive every npm version from the release
  tag and verify it against Cargo metadata before packaging.
- **Supply-chain credential exposure:** use npm OIDC on GitHub-hosted runners,
  automatic provenance, and no long-lived publish secret.
- **Large installs:** use exact platform optional dependencies instead of one
  all-platform package.
- **Cross-build host selecting the wrong optional package:** make target package
  inclusion explicit and test every advertised runner architecture.
- **Windows executable naming and Unix mode loss:** encode both in the platform
  table and assert them from the packed tarballs.
- **Registry propagation delay:** wait for every platform package before
  publishing the facade.

## Non-Goals

- Replacing mise as the recommended Treeboot installation method for direct
  project use.
- Exposing `treeboot-core` through N-API, WASM, FFI, or a reimplemented
  JavaScript API.
- Downloading GitHub Release assets during `postinstall` or first use.
- Shipping every platform binary in the facade package.
- Android npm packages in the first release.
- Supporting Node.js before version 22, Deno, browsers, or Electron renderers.
- Automatically configuring an application's Electron packager.
- Changing the existing macOS signing/notarization scope.
- Publishing from pull requests or manual workflow dispatches.
- Creating a separate npm version stream or release-please component.

## Pull Request Acceptance Criteria

- `treeboot` exposes documented ESM, CommonJS, types, and CLI entrypoints on
  Node.js 22 or newer.
- The six platform packages select and execute the existing release binaries on
  every advertised desktop architecture.
- npm and Bun installs work without install scripts or network fetches outside
  normal registry package retrieval.
- Electron packaging constraints and ASAR behavior are documented and tested at
  the proportionate level described above.
- All seven tarballs are built once, verified, and carried unchanged into the
  publish job.
- Tag publication uses npm Trusted Publishing with no npm token, publishes the
  facade last, and is safe to rerun.
- Manual release dispatch produces the npm artifacts without publishing them.
- The spec, architecture, README, release guide, roadmap, and package READMEs
  describe the same final contract.
- Focused checks, the six-platform package matrix, `mise run release:check`, and
  `mise run verify` pass on the intended final commit.
- Two independent final reviews and the repository's normal PR gates cover the
  exact final commit.

## Deferred Decisions

No decision blocks implementation. During the PR, choose the smallest accurate
examples for the Electron packagers actually verified in the manual smoke test.
Android packages and stronger Electron fixture coverage should be proposed only
after the six desktop packages have real usage evidence.

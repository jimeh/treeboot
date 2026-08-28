# treeboot for Node.js

This package makes the Treeboot executable available to Node.js tools and
Electron applications. For direct use in a project, install Treeboot with mise
instead. See the [Treeboot repository](https://github.com/jimeh/treeboot) for
the main installation instructions.

```sh
bun add treeboot
```

```ts
import { getBinaryPath, spawnTreeboot } from "treeboot";

console.log(getBinaryPath());
const child = spawnTreeboot(["status", "--json"], { stdio: "inherit" });
```

The package supports Node.js 22 and newer. It publishes ESM, CommonJS, and
TypeScript declarations.

## API

- `getBinaryPath()` returns the absolute path to the packaged executable for the
  current platform and architecture.
- `spawnTreeboot(args?, options?)` starts Treeboot and returns a Node.js
  `ChildProcess`.
- `spawnTreebootSync(args?, options?)` runs Treeboot synchronously and returns
  `SpawnSyncReturns<string | Buffer>`.
- `TreebootSpawnOptions` and `TreebootSpawnSyncOptions` match the corresponding
  Node.js spawn options without the `shell` field.

Both spawn functions pass arguments as separate strings and force
`shell: false`.

## Resolution and errors

The facade selects one exact-version optional dependency:

| Platform | Architecture | Package                     |
| -------- | ------------ | --------------------------- |
| macOS    | ARM64        | `@treeboot-rs/darwin-arm64` |
| macOS    | x64          | `@treeboot-rs/darwin-x64`   |
| Linux    | ARM64        | `@treeboot-rs/linux-arm64`  |
| Linux    | x64          | `@treeboot-rs/linux-x64`    |
| Windows  | ARM64        | `@treeboot-rs/win32-arm64`  |
| Windows  | x64          | `@treeboot-rs/win32-x64`    |

Resolution never downloads a binary and never falls back to an executable on
`PATH`.

The package exports these errors:

- `TreebootBinaryError` is the base class for packaged-binary resolution
  failures. Catch it to handle either public subclass.
- `UnsupportedPlatformError` reports an unsupported `platform` and `arch`.
- `MissingPlatformPackageError` reports the expected `packageName`, `platform`,
  and `arch`. Its inherited `cause` preserves the underlying package or binary
  resolution failure when one is available.

## CLI

`npx treeboot` and `bunx treeboot` run the packaged executable:

```sh
npx treeboot --version
bunx treeboot --version
```

The shim resolves the same platform package as the API, inherits standard input,
output, and error, and forwards `SIGINT` and `SIGTERM`. It exits with the
child's exit status, or with `128 + signal` when the child exits due to a
signal.

## Electron

Call Treeboot from Electron's main process, a utility process with Node.js
access, or an application backend. Renderer processes are not supported.

The selected `@treeboot-rs/*` package must remain in the packaged application.
Its `bin/` directory must be unpacked from `app.asar`, because Node's
`child_process.spawn` requires a real filesystem path. For example, configure
your packager to unpack:

```text
node_modules/@treeboot-rs/**/bin/**
```

The resolver uses an existing `.asar.unpacked` peer when it finds a resolved
binary inside `.asar`. Cross-architecture and universal builds must explicitly
install and retain the platform packages for every target architecture; host
optional-dependency selection is not enough.

Bundlers must leave the platform package as a runtime dependency. If optional
dependencies are disabled or pruned, `getBinaryPath()` throws
`MissingPlatformPackageError` with the expected package name.

See Electron's
[ASAR archive documentation](https://www.electronjs.org/docs/latest/tutorial/asar-archives#executing-binaries-inside-asar-archive)
for the underlying executable restrictions.

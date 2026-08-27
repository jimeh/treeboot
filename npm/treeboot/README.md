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

The package supports Node.js 22 and newer. `npx treeboot` and `bunx treeboot`
also run the packaged CLI.

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

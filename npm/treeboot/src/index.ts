import type { ChildProcess, SpawnSyncReturns } from "node:child_process";
import { createRequire } from "node:module";

import { resolveBinaryPath } from "./platform.js";
import {
  spawnBinary,
  spawnBinarySync,
  type SafeSpawnOptions,
  type SafeSpawnSyncOptions,
} from "./process.js";

export {
  MissingPlatformPackageError,
  TreebootBinaryError,
  UnsupportedPlatformError,
} from "./errors.js";

/** Options accepted by {@link spawnTreeboot}. Shell execution is forbidden. */
export type TreebootSpawnOptions = SafeSpawnOptions;

/** Options accepted by {@link spawnTreebootSync}. Shell execution is forbidden. */
export type TreebootSpawnSyncOptions = SafeSpawnSyncOptions;

const packageRequire = createRequire(import.meta.url);

/** Return the absolute path to the packaged Treeboot executable. */
export function getBinaryPath(): string {
  return resolveBinaryPath(
    process.platform,
    process.arch,
    packageRequire.resolve,
  );
}

/** Start Treeboot without invoking a shell. */
export function spawnTreeboot(
  args: readonly string[] = [],
  options: TreebootSpawnOptions = {},
): ChildProcess {
  return spawnBinary(getBinaryPath(), args, options);
}

/** Run Treeboot synchronously without invoking a shell. */
export function spawnTreebootSync(
  args: readonly string[] = [],
  options: TreebootSpawnSyncOptions = {},
): SpawnSyncReturns<string | Buffer> {
  return spawnBinarySync(getBinaryPath(), args, options);
}

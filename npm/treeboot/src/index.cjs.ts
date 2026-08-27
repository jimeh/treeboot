import type { ChildProcess, SpawnSyncReturns } from "node:child_process";

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

export type TreebootSpawnOptions = SafeSpawnOptions;
export type TreebootSpawnSyncOptions = SafeSpawnSyncOptions;

export function getBinaryPath(): string {
  return resolveBinaryPath(process.platform, process.arch, require.resolve);
}

export function spawnTreeboot(
  args: readonly string[] = [],
  options: TreebootSpawnOptions = {},
): ChildProcess {
  return spawnBinary(getBinaryPath(), args, options);
}

export function spawnTreebootSync(
  args: readonly string[] = [],
  options: TreebootSpawnSyncOptions = {},
): SpawnSyncReturns<string | Buffer> {
  return spawnBinarySync(getBinaryPath(), args, options);
}

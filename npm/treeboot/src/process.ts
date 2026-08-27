import {
  spawn,
  spawnSync,
  type ChildProcess,
  type SpawnOptions,
  type SpawnSyncOptions,
  type SpawnSyncReturns,
} from "node:child_process";

export type SafeSpawnOptions = Omit<SpawnOptions, "shell">;
export type SafeSpawnSyncOptions = Omit<SpawnSyncOptions, "shell">;

type Spawn = (
  command: string,
  args: readonly string[],
  options: SpawnOptions,
) => ChildProcess;
type SpawnSync = (
  command: string,
  args: readonly string[],
  options: SpawnSyncOptions,
) => SpawnSyncReturns<string | Buffer>;

export function spawnBinary(
  binaryPath: string,
  args: readonly string[],
  options: SafeSpawnOptions,
  spawnImplementation: Spawn = spawn as Spawn,
): ChildProcess {
  return spawnImplementation(binaryPath, [...args], {
    ...options,
    shell: false,
  });
}

export function spawnBinarySync(
  binaryPath: string,
  args: readonly string[],
  options: SafeSpawnSyncOptions,
  spawnImplementation: SpawnSync = spawnSync as SpawnSync,
): SpawnSyncReturns<string | Buffer> {
  return spawnImplementation(binaryPath, [...args], {
    ...options,
    shell: false,
  }) as SpawnSyncReturns<string | Buffer>;
}

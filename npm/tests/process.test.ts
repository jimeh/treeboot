import { describe, expect, test } from "bun:test";
import type {
  ChildProcess,
  SpawnOptions,
  SpawnSyncOptions,
  SpawnSyncReturns,
} from "node:child_process";

import { spawnBinary, spawnBinarySync } from "../treeboot/src/process.ts";

describe("process wrappers", () => {
  test("passes arguments verbatim and forces shell off", () => {
    const calls: unknown[][] = [];
    const fakeSpawn = (
      command: string,
      args: readonly string[],
      options: SpawnOptions,
    ): ChildProcess => {
      calls.push([command, args, options]);
      return {} as ChildProcess;
    };

    spawnBinary(
      "/tmp/treeboot",
      ["argument with spaces", "$(touch nope)", "; exit 7"],
      { cwd: "/tmp" },
      fakeSpawn,
    );

    expect(calls).toEqual([
      [
        "/tmp/treeboot",
        ["argument with spaces", "$(touch nope)", "; exit 7"],
        { cwd: "/tmp", shell: false },
      ],
    ]);
  });

  test("sync execution also forces shell off", () => {
    let receivedOptions: SpawnSyncOptions | undefined;
    const fakeSpawnSync = (
      _command: string,
      _args: readonly string[],
      options: SpawnSyncOptions,
    ): SpawnSyncReturns<Buffer> => {
      receivedOptions = options;
      return {
        output: [null, Buffer.alloc(0), Buffer.alloc(0)],
        pid: 1,
        signal: null,
        status: 0,
        stderr: Buffer.alloc(0),
        stdout: Buffer.alloc(0),
      } as SpawnSyncReturns<Buffer>;
    };

    spawnBinarySync("/tmp/treeboot", ["status"], {}, fakeSpawnSync);
    expect(receivedOptions?.shell).toBe(false);
  });
});

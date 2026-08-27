#!/usr/bin/env node

import { fileURLToPath } from "node:url";
import { constants } from "node:os";

import { spawnTreeboot } from "./index.js";

const FORWARDED_SIGNALS: readonly NodeJS.Signals[] = ["SIGINT", "SIGTERM"];
export interface CliSignalTarget {
  once(signal: NodeJS.Signals, listener: () => void): void;
  removeListener(signal: NodeJS.Signals, listener: () => void): void;
}

const signalProcess = process as unknown as CliSignalTarget;

type SpawnTreeboot = typeof spawnTreeboot;

export async function runCli(
  args: readonly string[],
  spawnImplementation: SpawnTreeboot = spawnTreeboot,
  signalTarget: CliSignalTarget = signalProcess,
): Promise<number> {
  const child = spawnImplementation(args, { stdio: "inherit" });
  const forwarders = new Map<NodeJS.Signals, () => void>();

  for (const signal of FORWARDED_SIGNALS) {
    const forward = (): void => {
      child.kill(signal);
    };
    forwarders.set(signal, forward);
    signalTarget.once(signal, forward);
  }

  try {
    return await new Promise<number>((resolve, reject) => {
      child.once("error", reject);
      child.once("exit", (code, signal) => {
        if (code !== null) {
          resolve(code);
          return;
        }
        resolve(signal === null ? 1 : 128 + signalNumber(signal));
      });
    });
  } finally {
    for (const [signal, forward] of forwarders) {
      signalTarget.removeListener(signal, forward);
    }
  }
}

function signalNumber(signal: NodeJS.Signals): number {
  return constants.signals[signal] ?? 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  runCli(process.argv.slice(2)).then(
    (code) => {
      process.exitCode = code;
    },
    (error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      process.stderr.write(`treeboot: ${message}\n`);
      process.exitCode = 1;
    },
  );
}

import { expect, test } from "bun:test";
import { EventEmitter } from "node:events";
import type { ChildProcess } from "node:child_process";

import { runCli } from "../treeboot/src/cli.ts";

test("CLI inherits stdio and returns the child exit status", async () => {
  const child = new EventEmitter() as ChildProcess;
  child.kill = () => true;
  let received: unknown[] = [];

  const result = runCli(["--version"], (args, options) => {
    received = [args, options];
    queueMicrotask(() => child.emit("exit", 23, null));
    return child;
  });

  expect(await result).toBe(23);
  expect(received).toEqual([["--version"], { stdio: "inherit" }]);
});

test("CLI converts a child signal to the conventional exit status", async () => {
  const child = new EventEmitter() as ChildProcess;
  child.kill = () => true;
  const result = runCli([], () => {
    queueMicrotask(() => child.emit("exit", null, "SIGTERM"));
    return child;
  });

  expect(await result).toBe(143);
});

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  test(`CLI forwards ${signal} and removes its signal listeners`, async () => {
    const child = new EventEmitter() as ChildProcess;
    const killedWith: NodeJS.Signals[] = [];
    child.kill = (receivedSignal) => {
      killedWith.push(receivedSignal as NodeJS.Signals);
      return true;
    };
    const signalTarget = new EventEmitter();
    const result = runCli([], () => child, signalTarget);

    signalTarget.emit(signal);
    child.emit("exit", 0, null);

    expect(await result).toBe(0);
    expect(killedWith).toEqual([signal]);
    expect(signalTarget.listenerCount("SIGINT")).toBe(0);
    expect(signalTarget.listenerCount("SIGTERM")).toBe(0);
  });
}

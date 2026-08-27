import { describe, expect, test } from "bun:test";

import {
  isolateTreebootPath,
  treebootShims,
} from "../scripts/path-isolation.ts";

describe("Bun launcher PATH isolation", () => {
  test("keeps the local bin first and removes POSIX entries with treeboot", () => {
    const existing = new Set(["/mise/bin/treeboot"]);

    expect(
      isolateTreebootPath(
        "/project/node_modules/.bin",
        "/usr/bin:/mise/bin:/project/node_modules/.bin:/opt/bin",
        "posix",
        (path) => existing.has(path),
      ),
    ).toBe("/project/node_modules/.bin:/usr/bin:/opt/bin");
  });

  test("handles Windows shims, separators, quoting, and casing", () => {
    const existing = new Set(["C:\\mise\\bin\\treeboot.cmd"]);

    expect(
      isolateTreebootPath(
        "C:\\project\\node_modules\\.bin",
        'C:\\Windows;"C:\\mise\\bin";c:\\PROJECT\\node_modules\\.bin;D:\\tools',
        "win32",
        (path) => existing.has(path),
      ),
    ).toBe("C:\\project\\node_modules\\.bin;C:\\Windows;D:\\tools");
  });

  test("finds all local executable and wrapper forms", () => {
    const existing = new Set([
      "C:\\project\\node_modules\\.bin\\treeboot.exe",
      "C:\\project\\node_modules\\.bin\\treeboot.ps1",
    ]);

    expect(
      treebootShims("C:\\project\\node_modules\\.bin", "win32", (path) =>
        existing.has(path),
      ),
    ).toEqual([...existing]);
  });
});

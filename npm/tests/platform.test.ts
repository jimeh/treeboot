import { describe, expect, test } from "bun:test";

import {
  MissingPlatformPackageError,
  UnsupportedPlatformError,
} from "../treeboot/src/errors.ts";
import {
  getPlatformPackage,
  preferAsarUnpacked,
  resolveBinaryPath,
} from "../treeboot/src/platform.ts";

describe("platform package mapping", () => {
  const cases = [
    ["darwin", "arm64", "@treeboot-rs/darwin-arm64", "treeboot"],
    ["darwin", "x64", "@treeboot-rs/darwin-x64", "treeboot"],
    ["linux", "arm64", "@treeboot-rs/linux-arm64", "treeboot"],
    ["linux", "x64", "@treeboot-rs/linux-x64", "treeboot"],
    ["win32", "arm64", "@treeboot-rs/win32-arm64", "treeboot.exe"],
    ["win32", "x64", "@treeboot-rs/win32-x64", "treeboot.exe"],
  ] as const;

  for (const [platform, arch, packageName, executableName] of cases) {
    test(`${platform}/${arch}`, () => {
      expect(getPlatformPackage(platform, arch)).toEqual({
        executableName,
        packageName,
      });
    });
  }

  test("rejects an unsupported architecture with a typed error", () => {
    expect(() => getPlatformPackage("linux", "riscv64")).toThrow(
      UnsupportedPlatformError,
    );
  });
});

describe("binary resolution", () => {
  test("resolves the executable relative to the platform manifest", () => {
    const manifest = "/app/node_modules/@treeboot-rs/linux-x64/package.json";
    const binary = "/app/node_modules/@treeboot-rs/linux-x64/bin/treeboot";
    expect(
      resolveBinaryPath(
        "linux",
        "x64",
        () => manifest,
        (path) => path === binary,
      ),
    ).toBe(binary);
  });

  test("reports a pruned optional dependency", () => {
    const resolve = (): string => {
      throw new Error("MODULE_NOT_FOUND");
    };
    expect(() => resolveBinaryPath("linux", "x64", resolve)).toThrow(
      MissingPlatformPackageError,
    );
    try {
      resolveBinaryPath("linux", "x64", resolve);
    } catch (error) {
      expect(error).toBeInstanceOf(MissingPlatformPackageError);
      expect((error as MissingPlatformPackageError).packageName).toBe(
        "@treeboot-rs/linux-x64",
      );
      expect((error as Error).message).toContain("optional dependencies");
    }
  });

  test("reports a package whose executable is missing", () => {
    expect(() =>
      resolveBinaryPath(
        "win32",
        "arm64",
        () => "C:\\app\\node_modules\\@treeboot-rs\\win32-arm64\\package.json",
        () => false,
      ),
    ).toThrow(MissingPlatformPackageError);
  });
});

describe("Electron ASAR resolution", () => {
  const archived =
    "/opt/MyApp/resources/app.asar/node_modules/pkg/bin/treeboot";
  const unpacked =
    "/opt/MyApp/resources/app.asar.unpacked/node_modules/pkg/bin/treeboot";

  test("prefers an existing unpacked peer", () => {
    expect(preferAsarUnpacked(archived, (path) => path === unpacked)).toBe(
      unpacked,
    );
  });

  test("keeps the archive path when the unpacked peer is absent", () => {
    expect(preferAsarUnpacked(archived, () => false)).toBe(archived);
  });

  test("supports Windows separators", () => {
    const windowsArchived =
      "C:\\app\\resources\\app.asar\\node_modules\\pkg\\bin\\treeboot.exe";
    const windowsUnpacked = windowsArchived.replace(
      "app.asar",
      "app.asar.unpacked",
    );
    expect(
      preferAsarUnpacked(windowsArchived, (path) => path === windowsUnpacked),
    ).toBe(windowsUnpacked);
  });
});

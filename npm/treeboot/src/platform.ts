import { dirname, join } from "node:path";
import { existsSync } from "node:fs";

import {
  MissingPlatformPackageError,
  UnsupportedPlatformError,
} from "./errors.js";

export interface PlatformPackage {
  readonly packageName: string;
  readonly executableName: string;
}

type Resolve = (specifier: string) => string;

const PLATFORM_PACKAGES: Readonly<
  Partial<Record<NodeJS.Platform, Readonly<Record<string, PlatformPackage>>>>
> = {
  darwin: {
    arm64: {
      packageName: "@treeboot-rs/darwin-arm64",
      executableName: "treeboot",
    },
    x64: {
      packageName: "@treeboot-rs/darwin-x64",
      executableName: "treeboot",
    },
  },
  linux: {
    arm64: {
      packageName: "@treeboot-rs/linux-arm64",
      executableName: "treeboot",
    },
    x64: {
      packageName: "@treeboot-rs/linux-x64",
      executableName: "treeboot",
    },
  },
  win32: {
    arm64: {
      packageName: "@treeboot-rs/win32-arm64",
      executableName: "treeboot.exe",
    },
    x64: {
      packageName: "@treeboot-rs/win32-x64",
      executableName: "treeboot.exe",
    },
  },
};

export function getPlatformPackage(
  platform: NodeJS.Platform,
  arch: string,
): PlatformPackage {
  const result = PLATFORM_PACKAGES[platform]?.[arch];
  if (result === undefined) {
    throw new UnsupportedPlatformError(platform, arch);
  }
  return result;
}

export function preferAsarUnpacked(
  binaryPath: string,
  pathExists: (path: string) => boolean = existsSync,
): string {
  const unpackedPath = binaryPath.replace(
    /([\\/][^\\/]+\.asar)([\\/])/i,
    "$1.unpacked$2",
  );
  if (unpackedPath !== binaryPath && pathExists(unpackedPath)) {
    return unpackedPath;
  }
  return binaryPath;
}

export function resolveBinaryPath(
  platform: NodeJS.Platform,
  arch: string,
  resolve: Resolve,
  pathExists: (path: string) => boolean = existsSync,
): string {
  const target = getPlatformPackage(platform, arch);
  try {
    const manifestPath = resolve(`${target.packageName}/package.json`);
    const binaryPath = join(
      dirname(manifestPath),
      "bin",
      target.executableName,
    );
    const runnablePath = preferAsarUnpacked(binaryPath, pathExists);
    if (!pathExists(runnablePath)) {
      throw new Error(`missing executable at ${runnablePath}`);
    }
    return runnablePath;
  } catch (error) {
    if (error instanceof MissingPlatformPackageError) {
      throw error;
    }
    throw new MissingPlatformPackageError(
      target.packageName,
      platform,
      arch,
      error,
    );
  }
}

import { existsSync } from "node:fs";
import { posix, win32 } from "node:path";

const shimNames = ["treeboot", "treeboot.cmd", "treeboot.exe", "treeboot.ps1"];

type PathPlatform = "posix" | "win32";

export function treebootShims(
  binDirectory: string,
  platform: PathPlatform = process.platform === "win32" ? "win32" : "posix",
  pathExists: (path: string) => boolean = existsSync,
): string[] {
  const paths = platform === "win32" ? win32 : posix;
  return shimNames
    .map((name) => paths.join(binDirectory, name))
    .filter(pathExists);
}

export function isolateTreebootPath(
  localBinDirectory: string,
  originalPath: string,
  platform: PathPlatform = process.platform === "win32" ? "win32" : "posix",
  pathExists: (path: string) => boolean = existsSync,
): string {
  const paths = platform === "win32" ? win32 : posix;
  const normalize = (entry: string): string => {
    const unquoted = entry.replace(/^"(.*)"$/, "$1");
    const normalized = paths.normalize(unquoted);
    return platform === "win32" ? normalized.toLowerCase() : normalized;
  };
  const localBin = normalize(localBinDirectory);
  const safeEntries = originalPath
    .split(paths.delimiter)
    .filter((entry) => entry.length > 0)
    .filter((entry) => normalize(entry) !== localBin)
    .filter(
      (entry) =>
        !shimNames.some((name) =>
          pathExists(paths.join(entry.replace(/^"(.*)"$/, "$1"), name)),
        ),
    );

  return [localBinDirectory, ...safeEntries].join(paths.delimiter);
}

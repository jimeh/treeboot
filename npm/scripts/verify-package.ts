import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import type { PackageManifest } from "./manifest.ts";
import {
  facadePackageName,
  packageFilename,
  platformPackages,
} from "./packages.ts";
import { readTarGzip, type TarEntry } from "./tar.ts";

const root = fileURLToPath(new URL("../..", import.meta.url));

if (import.meta.main) {
  const outputDirectory = parseOutputDirectory(process.argv.slice(2));
  await verifyPackages(outputDirectory);
}

export async function verifyPackages(
  outputDirectory: string,
  options: { checkUnixModes?: boolean } = {},
): Promise<void> {
  const absoluteOutput = resolve(root, outputDirectory);
  const manifest = JSON.parse(
    await readFile(join(absoluteOutput, "manifest.json"), "utf8"),
  ) as PackageManifest;
  const expectedNames: string[] = platformPackages.map(
    ({ npmName }) => npmName,
  );
  if (manifest.packages.some(({ name }) => name === facadePackageName)) {
    expectedNames.push(facadePackageName);
  }
  assertEqual(manifest.packages.length, expectedNames.length, "package count");
  assertEqual(
    manifest.version,
    manifest.packages[0]?.version,
    "manifest version",
  );

  for (const name of expectedNames) {
    const artifact = manifest.packages.find(
      (candidate) => candidate.name === name,
    );
    if (artifact === undefined) {
      throw new Error(`Missing artifact entry for ${name}`);
    }
    assertEqual(artifact.version, manifest.version, `${name} version`);
    assertEqual(
      artifact.filename,
      packageFilename(name, manifest.version),
      `${name} filename`,
    );
    const tarball = await readFile(
      join(absoluteOutput, "tarballs", artifact.filename),
    );
    assertEqual(tarball.length, artifact.size, `${name} size`);
    assertEqual(
      `sha512-${createHash("sha512").update(tarball).digest("base64")}`,
      artifact.integrity,
      `${name} integrity`,
    );
    verifyTarball(
      name,
      manifest.version,
      readTarGzip(tarball),
      options.checkUnixModes ?? true,
    );
  }
  console.log(
    `Verified ${manifest.packages.length} npm packages for ${manifest.version}`,
  );
}

export function verifyTarball(
  name: string,
  version: string,
  entries: readonly TarEntry[],
  checkUnixModes: boolean,
): void {
  const byName = new Map(entries.map((entry) => [entry.name, entry]));
  const manifestEntry = byName.get("package/package.json");
  if (manifestEntry === undefined) {
    throw new Error(`${name}: package.json is missing`);
  }
  const packageJson = JSON.parse(
    manifestEntry.bytes.toString("utf8"),
  ) as Record<string, unknown>;
  assertEqual(packageJson.name, name, `${name} package name`);
  assertEqual(packageJson.version, version, `${name} package version`);
  assertEqual(packageJson.private, undefined, `${name} private flag`);
  assertEqual(packageJson.scripts, undefined, `${name} lifecycle scripts`);
  assertEqual(packageJson.license, "MIT", `${name} license`);
  assertEqual(
    (packageJson.publishConfig as { access?: unknown } | undefined)?.access,
    "public",
    `${name} publish access`,
  );

  const common = [
    "package/LICENSE",
    "package/README.md",
    "package/package.json",
  ];
  if (name === facadePackageName) {
    const expectedFiles = [
      ...common,
      "package/dist/cli.js",
      "package/dist/index.cjs",
      "package/dist/index.js",
      "package/dist/types/index.d.cts",
      "package/dist/types/cli.d.ts",
      "package/dist/types/errors.d.ts",
      "package/dist/types/index.d.ts",
      "package/dist/types/platform.d.ts",
      "package/dist/types/process.d.ts",
    ];
    assertFiles(entries, expectedFiles, name);
    assertEqual(packageJson.type, "module", `${name} package type`);
    assertEqual(
      packageJson.main,
      "./dist/index.cjs",
      `${name} CommonJS fallback`,
    );
    assertEqual(
      packageJson.types,
      "./dist/types/index.d.ts",
      `${name} declaration fallback`,
    );
    assertEqual(
      packageJson.bin,
      { treeboot: "./dist/cli.js" },
      `${name} executable mapping`,
    );
    assertEqual(
      packageJson.exports,
      {
        ".": {
          import: {
            types: "./dist/types/index.d.ts",
            default: "./dist/index.js",
          },
          require: {
            types: "./dist/types/index.d.cts",
            default: "./dist/index.cjs",
          },
        },
      },
      `${name} exports`,
    );
    assertEqual(
      packageJson.engines,
      { node: ">=22" },
      `${name} Node.js engine`,
    );
    const optionalDependencies = packageJson.optionalDependencies as
      | Record<string, unknown>
      | undefined;
    assertEqual(
      optionalDependencies,
      Object.fromEntries(
        platformPackages.map(({ npmName }) => [npmName, version]),
      ),
      `${name} optional dependencies`,
    );
    assertExecutable(byName.get("package/dist/cli.js"), name);
    const cjsContents = byName
      .get("package/dist/index.cjs")
      ?.bytes.toString("utf8");
    if (cjsContents?.includes("/npm/treeboot/src/") === true) {
      throw new Error(`${name}: CommonJS build embeds a source checkout path`);
    }
    return;
  }

  const platformPackage = platformPackages.find(
    ({ npmName }) => npmName === name,
  );
  if (platformPackage === undefined) {
    throw new Error(`Unknown npm package ${name}`);
  }
  assertEqual(packageJson.os, [platformPackage.os], `${name} os`);
  assertEqual(packageJson.cpu, [platformPackage.arch], `${name} cpu`);
  assertEqual(packageJson.libc, undefined, `${name} libc restriction`);
  assertEqual(
    packageJson.exports,
    { "./package.json": "./package.json" },
    `${name} exports`,
  );
  assertEqual(packageJson.dependencies, undefined, `${name} dependencies`);
  assertEqual(
    packageJson.optionalDependencies,
    undefined,
    `${name} optional dependencies`,
  );
  const binaryName = `package/bin/${platformPackage.executableName}`;
  const hasBinary = byName.has(binaryName);
  if (!hasBinary && version !== "0.0.0") {
    throw new Error(`${name}: missing packaged executable ${binaryName}`);
  }
  assertFiles(entries, hasBinary ? [...common, binaryName] : common, name);
  if (hasBinary && platformPackage.os !== "win32" && checkUnixModes) {
    assertExecutable(byName.get(binaryName), name);
  }
}

function assertExecutable(
  entry: TarEntry | undefined,
  packageName: string,
): void {
  if (entry === undefined || (entry.mode & 0o111) === 0) {
    throw new Error(`${packageName}: expected executable archive mode`);
  }
}

function assertFiles(
  entries: readonly TarEntry[],
  expected: readonly string[],
  packageName: string,
): void {
  const actual = entries.map(({ name }) => name).sort();
  const sortedExpected = [...expected].sort();
  assertEqual(actual, sortedExpected, `${packageName} file allowlist`);
}

function assertEqual(actual: unknown, expected: unknown, label: string): void {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
    );
  }
}

function parseOutputDirectory(args: readonly string[]): string {
  if (args.length === 0) {
    return "npm-dist";
  }
  if (
    args.length === 2 &&
    args[0] === "--output-dir" &&
    args[1] !== undefined
  ) {
    return args[1];
  }
  throw new Error("Usage: verify-package.ts [--output-dir DIR]");
}

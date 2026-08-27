import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { basename, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

import { packageRelease } from "./package-release.ts";
import { facadePackageName, platformPackages } from "./packages.ts";
import { isolateTreebootPath, treebootShims } from "./path-isolation.ts";
import { verifyPackages } from "./verify-package.ts";

const root = fileURLToPath(new URL("../..", import.meta.url));

if (!import.meta.main) {
  throw new Error("smoke-host.ts is a command, not an importable module");
}

const binaryArgument = process.argv[2];
if (binaryArgument === undefined) {
  throw new Error("Usage: smoke-host.ts PATH_TO_HOST_TREEBOOT_BINARY");
}
const binaryPath = resolve(root, binaryArgument);
const platformPackage = platformPackages.find(
  ({ os, arch }) => os === process.platform && arch === process.arch,
);
if (platformPackage === undefined) {
  throw new Error(
    `No npm package for host ${process.platform}/${process.arch}`,
  );
}
const cargoContents = await readFile(join(root, "Cargo.toml"), "utf8");
const version = cargoContents.match(/^version = "([^"]+)"/m)?.[1];
if (version === undefined) {
  throw new Error("Could not read Cargo package version");
}

const temporaryDirectory = await mkdtemp(join(tmpdir(), "treeboot-npm-smoke-"));
try {
  const assetsDirectory = join(temporaryDirectory, "assets");
  const outputDirectory = join(temporaryDirectory, "packages");
  await mkdir(assetsDirectory);
  for (const definition of platformPackages) {
    const target = join(assetsDirectory, definition.assetName);
    if (definition === platformPackage) {
      await copyFile(binaryPath, target);
    } else {
      await writeFile(target, "cross-platform fixture\n");
    }
  }
  await packageRelease({
    assetsDirectory,
    outputDirectory,
    placeholder: false,
    version,
  });
  await verifyPackages(outputDirectory, {
    checkUnixModes: process.platform !== "win32",
  });

  const manifest = JSON.parse(
    await readFile(join(outputDirectory, "manifest.json"), "utf8"),
  ) as {
    packages: { filename: string; name: string }[];
  };
  const facade = tarballFor(
    manifest.packages,
    facadePackageName,
    outputDirectory,
  );
  const platform = tarballFor(
    manifest.packages,
    platformPackage.npmName,
    outputDirectory,
  );

  await smokeInstall("npm", [
    "install",
    "--ignore-scripts",
    "--no-audit",
    "--no-fund",
    platform,
    facade,
  ]);
  await smokeInstall("bun", ["add", "--ignore-scripts", platform, facade]);
  await smokeMissingOptional(facade);
  console.log(
    `Host npm smoke passed for ${platformPackage.npmName} using ${basename(binaryPath)}`,
  );
} finally {
  await rm(temporaryDirectory, { force: true, recursive: true });
}

async function smokeInstall(
  manager: "bun" | "npm",
  installArguments: readonly string[],
): Promise<void> {
  const directory = join(temporaryDirectory, `install-${manager}`);
  await mkdir(directory);
  await writeFile(
    join(directory, "package.json"),
    '{"name":"treeboot-smoke","private":true,"type":"module"}\n',
  );
  await run([manager, ...installArguments], directory);
  await run(
    [
      "node",
      "--input-type=module",
      "--eval",
      'import { getBinaryPath, spawnTreebootSync } from "treeboot"; ' +
        "const path = getBinaryPath(); " +
        "const result = spawnTreebootSync(['--version'], { encoding: 'utf8' }); " +
        "if (!path || result.status !== 0 || !result.stdout.includes('treeboot')) process.exit(1);",
    ],
    directory,
  );
  await run(
    [
      "node",
      "--eval",
      "const api = require('treeboot'); " +
        "if (!api.getBinaryPath().includes('@treeboot-rs')) process.exit(1);",
    ],
    directory,
  );
  if (manager === "npm") {
    await writeFile(
      join(directory, "consumer.cts"),
      'import treeboot = require("treeboot");\n' +
        "const binaryPath: string = treeboot.getBinaryPath();\n" +
        "treeboot.spawnTreeboot([binaryPath]);\n",
    );
    await run(
      [
        "node",
        join(root, "node_modules", "typescript", "bin", "tsc"),
        "--noEmit",
        "--strict",
        "--module",
        "NodeNext",
        "--moduleResolution",
        "NodeNext",
        "--target",
        "ES2022",
        "--types",
        "node",
        "--typeRoots",
        join(root, "node_modules", "@types"),
        "consumer.cts",
      ],
      directory,
    );
  }
  const localBinDirectory = join(directory, "node_modules", ".bin");
  const localShims = treebootShims(localBinDirectory);
  if (localShims.length === 0) {
    throw new Error(`${manager} installed no local treeboot executable shim`);
  }
  const environment =
    manager === "bun" ? isolatedBunEnvironment(localBinDirectory) : undefined;
  const launcher =
    manager === "npm"
      ? ["npx", "--no-install", "treeboot", "--version"]
      : [process.execPath, "x", "--no-install", "treeboot", "--version"];
  const versionOutput = await runWithOutput(launcher, directory, environment);
  if (!/^treeboot \S+/m.test(versionOutput)) {
    throw new Error(
      `${manager} installed treeboot shim returned no version output`,
    );
  }
  console.log(versionOutput.trim());

  if (manager === "bun") {
    if (environment === undefined) {
      throw new Error("Bun launcher environment was not isolated");
    }
    await assertMissingLocalBunShimFails(directory, localShims, environment);
  }
}

function isolatedBunEnvironment(
  localBinDirectory: string,
): Record<string, string | undefined> {
  const environment = { ...process.env };
  const pathKeys = Object.keys(environment).filter(
    (key) => key.toLowerCase() === "path",
  );
  const originalPath =
    pathKeys
      .map((key) => environment[key])
      .find((value) => value !== undefined) ?? "";
  for (const key of pathKeys) {
    delete environment[key];
  }
  environment[process.platform === "win32" ? "Path" : "PATH"] =
    isolateTreebootPath(localBinDirectory, originalPath);
  return environment;
}

async function assertMissingLocalBunShimFails(
  directory: string,
  localShims: readonly string[],
  environment: Record<string, string | undefined>,
): Promise<void> {
  const disabledShims = localShims.map((path) => `${path}.disabled`);
  for (const [index, path] of localShims.entries()) {
    await rename(path, disabledShims[index]!);
  }
  try {
    const child = Bun.spawn(
      [process.execPath, "x", "--no-install", "treeboot", "--version"],
      { cwd: directory, env: environment, stderr: "pipe", stdout: "pipe" },
    );
    const status = await child.exited;
    if (status === 0) {
      throw new Error(
        "Bun launcher resolved treeboot after every local shim was disabled",
      );
    }
    console.log(
      "Bun launcher rejected global fallback with every local shim disabled",
    );
  } finally {
    for (const [index, path] of localShims.entries()) {
      await rename(disabledShims[index]!, path);
    }
  }
}

async function smokeMissingOptional(facade: string): Promise<void> {
  const directory = join(temporaryDirectory, "install-missing-optional");
  await mkdir(directory);
  await writeFile(
    join(directory, "package.json"),
    '{"name":"treeboot-missing-smoke","private":true,"type":"module"}\n',
  );
  await run(
    [
      "npm",
      "install",
      "--ignore-scripts",
      "--no-audit",
      "--no-fund",
      "--omit=optional",
      facade,
    ],
    directory,
  );
  await run(
    [
      "node",
      "--input-type=module",
      "--eval",
      'import { getBinaryPath, MissingPlatformPackageError } from "treeboot"; ' +
        "try { getBinaryPath(); process.exit(1); } " +
        "catch (error) { if (!(error instanceof MissingPlatformPackageError)) process.exit(1); }",
    ],
    directory,
  );
}

async function run(command: readonly string[], cwd: string): Promise<void> {
  const child = Bun.spawn([...command], {
    cwd,
    stderr: "inherit",
    stdout: "inherit",
  });
  const status = await child.exited;
  if (status !== 0) {
    throw new Error(`${command[0]} failed with status ${status}`);
  }
}

async function runWithOutput(
  command: readonly string[],
  cwd: string,
  env?: Record<string, string | undefined>,
): Promise<string> {
  const child = Bun.spawn([...command], {
    cwd,
    ...(env === undefined ? {} : { env }),
    stderr: "pipe",
    stdout: "pipe",
  });
  const [status, stderr, stdout] = await Promise.all([
    child.exited,
    new Response(child.stderr).text(),
    new Response(child.stdout).text(),
  ]);
  if (status !== 0) {
    throw new Error(
      `${command[0]} failed with status ${status}: ${stderr.trim()}`,
    );
  }
  return stdout;
}

function tarballFor(
  packages: readonly { filename: string; name: string }[],
  name: string,
  outputDirectory: string,
): string {
  const artifact = packages.find((candidate) => candidate.name === name);
  if (artifact === undefined) {
    throw new Error(`Missing staged tarball for ${name}`);
  }
  return join(outputDirectory, "tarballs", artifact.filename);
}

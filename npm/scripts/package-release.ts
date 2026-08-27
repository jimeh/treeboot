import { createHash } from "node:crypto";
import {
  chmod,
  copyFile,
  cp,
  lstat,
  mkdir,
  readFile,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { dirname, join, parse, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import type { PackageArtifact, PackageManifest } from "./manifest.ts";
import {
  facadePackageName,
  packageFilename,
  platformPackages,
} from "./packages.ts";

export interface PackageReleaseOptions {
  readonly assetsDirectory: string;
  readonly outputDirectory: string;
  readonly placeholder: boolean;
  readonly version: string;
}

const root = fileURLToPath(new URL("../..", import.meta.url));
const outputMarker = ".treeboot-npm-dist";
const outputMarkerContents = "treeboot npm distribution output\n";

if (import.meta.main) {
  await packageRelease(parseOptions(process.argv.slice(2)));
}

export async function packageRelease(
  options: PackageReleaseOptions,
): Promise<PackageManifest> {
  await validateVersion(options.version, options.placeholder);
  const outputDirectory = resolve(root, options.outputDirectory);
  const stagingDirectory = join(outputDirectory, "staging");
  const tarballDirectory = join(outputDirectory, "tarballs");
  await prepareOutputDirectory(outputDirectory);
  await mkdir(stagingDirectory, { recursive: true });
  await mkdir(tarballDirectory, { recursive: true });

  const artifacts: PackageArtifact[] = [];
  for (const platformPackage of platformPackages) {
    const packageDirectory = join(stagingDirectory, platformPackage.workspace);
    const sourceDirectory = join(
      root,
      "npm",
      "platforms",
      platformPackage.workspace,
    );
    await mkdir(packageDirectory, { recursive: true });
    const manifest = await readJson(join(sourceDirectory, "package.json"));
    delete manifest.private;
    manifest.version = options.version;
    if (options.placeholder) {
      manifest.files = ["README.md", "LICENSE"];
      await writeFile(
        join(packageDirectory, "README.md"),
        "# Treeboot npm placeholder\n\n" +
          "Treeboot's npm distribution is not ready yet. Install Treeboot with mise for now.\n",
      );
    } else {
      const binDirectory = join(packageDirectory, "bin");
      await mkdir(binDirectory);
      await copyFile(
        resolve(root, options.assetsDirectory, platformPackage.assetName),
        join(binDirectory, platformPackage.executableName),
      );
      if (platformPackage.os !== "win32") {
        await chmod(join(binDirectory, platformPackage.executableName), 0o755);
      }
      await copyFile(
        join(sourceDirectory, "README.md"),
        join(packageDirectory, "README.md"),
      );
    }
    await copyFile(join(root, "LICENSE"), join(packageDirectory, "LICENSE"));
    await writeJson(join(packageDirectory, "package.json"), manifest);
    artifacts.push(
      await packDirectory(
        packageDirectory,
        platformPackage.npmName,
        options.version,
        tarballDirectory,
      ),
    );
  }

  if (!options.placeholder) {
    const sourceDirectory = join(root, "npm", "treeboot");
    const packageDirectory = join(stagingDirectory, facadePackageName);
    const manifest = await readJson(join(sourceDirectory, "package.json"));
    delete manifest.private;
    manifest.version = options.version;
    manifest.optionalDependencies = Object.fromEntries(
      platformPackages.map(({ npmName }) => [npmName, options.version]),
    );
    await mkdir(packageDirectory, { recursive: true });
    await cp(join(sourceDirectory, "dist"), join(packageDirectory, "dist"), {
      recursive: true,
    });
    await copyFile(
      join(sourceDirectory, "README.md"),
      join(packageDirectory, "README.md"),
    );
    await copyFile(join(root, "LICENSE"), join(packageDirectory, "LICENSE"));
    await writeJson(join(packageDirectory, "package.json"), manifest);
    artifacts.push(
      await packDirectory(
        packageDirectory,
        facadePackageName,
        options.version,
        tarballDirectory,
      ),
    );
  }

  const packageManifest: PackageManifest = {
    packages: artifacts,
    version: options.version,
  };
  await writeJson(join(outputDirectory, "manifest.json"), packageManifest);
  await rm(stagingDirectory, { recursive: true });
  console.log(
    `Packed ${artifacts.length} npm packages for ${options.version} in ${tarballDirectory}`,
  );
  return packageManifest;
}

async function prepareOutputDirectory(outputDirectory: string): Promise<void> {
  if (
    outputDirectory === root ||
    outputDirectory === parse(outputDirectory).root
  ) {
    throw new Error(`Refusing unsafe npm output directory: ${outputDirectory}`);
  }

  let existing: Awaited<ReturnType<typeof lstat>> | undefined;
  try {
    existing = await lstat(outputDirectory);
  } catch (error) {
    if (!isNodeError(error) || error.code !== "ENOENT") {
      throw error;
    }
  }

  if (existing !== undefined) {
    if (!existing.isDirectory() || existing.isSymbolicLink()) {
      throw new Error(
        `${outputDirectory} is not owned by the Treeboot npm packager`,
      );
    }
    let marker: string | undefined;
    try {
      marker = await readFile(join(outputDirectory, outputMarker), "utf8");
    } catch (error) {
      if (!isNodeError(error) || error.code !== "ENOENT") {
        throw error;
      }
    }
    if (marker !== outputMarkerContents) {
      throw new Error(
        `${outputDirectory} is not owned by the Treeboot npm packager`,
      );
    }
    await rm(outputDirectory, { recursive: true });
  }

  await mkdir(outputDirectory, { recursive: true });
  await writeFile(join(outputDirectory, outputMarker), outputMarkerContents);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

async function packDirectory(
  packageDirectory: string,
  name: string,
  version: string,
  outputDirectory: string,
): Promise<PackageArtifact> {
  const filename = packageFilename(name, version);
  const child = Bun.spawn(
    [
      process.execPath,
      "pm",
      "pack",
      "--ignore-scripts",
      "--quiet",
      "--filename",
      join(outputDirectory, filename),
    ],
    { cwd: packageDirectory, stderr: "inherit", stdout: "inherit" },
  );
  const status = await child.exited;
  if (status !== 0) {
    throw new Error(`Failed to pack ${name} with status ${status}`);
  }
  const tarballPath = join(outputDirectory, filename);
  const bytes = await readFile(tarballPath);
  return {
    filename,
    integrity: `sha512-${createHash("sha512").update(bytes).digest("base64")}`,
    name,
    size: (await stat(tarballPath)).size,
    version,
  };
}

function parseOptions(args: readonly string[]): PackageReleaseOptions {
  const values = new Map<string, string>();
  let placeholder = false;
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--placeholder") {
      placeholder = true;
      continue;
    }
    const value = args[index + 1];
    if (argument?.startsWith("--") && value !== undefined) {
      values.set(argument, value);
      index += 1;
      continue;
    }
    throw new Error(`Unexpected packaging argument: ${argument}`);
  }
  const version = values.get("--version");
  if (version === undefined) {
    throw new Error("--version is required");
  }
  return {
    assetsDirectory: values.get("--assets-dir") ?? "dist",
    outputDirectory: values.get("--output-dir") ?? "npm-dist",
    placeholder,
    version,
  };
}

async function validateVersion(
  version: string,
  placeholder: boolean,
): Promise<void> {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.test(version)) {
    throw new Error(`Invalid npm package version: ${version}`);
  }
  if (placeholder) {
    if (version !== "0.0.0") {
      throw new Error("Placeholder packages must use version 0.0.0");
    }
    return;
  }
  const cargoContents = await readFile(join(root, "Cargo.toml"), "utf8");
  const cargoVersion = cargoContents.match(/^version = "([^"]+)"/m)?.[1];
  if (
    cargoVersion === undefined ||
    (version !== cargoVersion && !version.startsWith(`${cargoVersion}-`))
  ) {
    throw new Error(
      `npm version ${version} does not match Cargo version ${cargoVersion ?? "unknown"}`,
    );
  }
}

async function readJson(path: string): Promise<Record<string, unknown>> {
  return JSON.parse(await readFile(path, "utf8")) as Record<string, unknown>;
}

async function writeJson(path: string, value: unknown): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`);
}

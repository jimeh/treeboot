import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import {
  appendFile,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { packageRelease } from "../scripts/package-release.ts";
import { platformPackages } from "../scripts/packages.ts";
import { publishPackages } from "../scripts/publish-packages.ts";
import { readTarGzip } from "../scripts/tar.ts";
import { verifyPackages, verifyTarball } from "../scripts/verify-package.ts";

let temporaryDirectory: string;
let assetsDirectory: string;

beforeAll(async () => {
  temporaryDirectory = await mkdtemp(join(tmpdir(), "treeboot-npm-test-"));
  assetsDirectory = join(temporaryDirectory, "assets");
  await mkdir(assetsDirectory);
  for (const { assetName } of platformPackages) {
    await writeFile(
      join(assetsDirectory, assetName),
      "fixture treeboot executable\n",
    );
  }
});

afterAll(async () => {
  await rm(temporaryDirectory, { force: true, recursive: true });
});

describe("release package staging", () => {
  test("packs and verifies the facade and six binary packages", async () => {
    const outputDirectory = join(temporaryDirectory, "release");
    const manifest = await packageRelease({
      assetsDirectory,
      outputDirectory,
      placeholder: false,
      version: "0.13.0",
    });

    expect(manifest.packages).toHaveLength(7);
    expect(manifest.packages.at(-1)?.name).toBe("treeboot");
    await verifyPackages(outputDirectory);
  });

  test("produces deterministic package bytes", async () => {
    const first = await packageRelease({
      assetsDirectory,
      outputDirectory: join(temporaryDirectory, "deterministic-a"),
      placeholder: false,
      version: "0.13.0",
    });
    const second = await packageRelease({
      assetsDirectory,
      outputDirectory: join(temporaryDirectory, "deterministic-b"),
      placeholder: false,
      version: "0.13.0",
    });

    expect(second.packages.map(({ integrity }) => integrity)).toEqual(
      first.packages.map(({ integrity }) => integrity),
    );
  });

  test("refuses to clear an existing unowned output directory", async () => {
    const outputDirectory = join(temporaryDirectory, "unowned");
    const sentinel = join(outputDirectory, "sentinel.txt");
    await mkdir(outputDirectory);
    await writeFile(sentinel, "keep me\n");

    await Promise.resolve(
      expect(
        packageRelease({
          assetsDirectory,
          outputDirectory,
          placeholder: false,
          version: "0.13.0",
        }),
      ).rejects.toThrow("not owned by the Treeboot npm packager"),
    );
    expect(await readFile(sentinel, "utf8")).toBe("keep me\n");
  });

  test("refuses the repository root as an unsafe output directory", async () => {
    const workspaceManifest = join(import.meta.dir, "../..", "package.json");
    const before = await readFile(workspaceManifest, "utf8");

    await Promise.resolve(
      expect(
        packageRelease({
          assetsDirectory,
          outputDirectory: ".",
          placeholder: false,
          version: "0.13.0",
        }),
      ).rejects.toThrow("Refusing unsafe npm output directory"),
    );

    expect(await readFile(workspaceManifest, "utf8")).toBe(before);
  });

  test("clears and recreates an owned output directory on rerun", async () => {
    const outputDirectory = join(temporaryDirectory, "rerun");
    await packageRelease({
      assetsDirectory,
      outputDirectory,
      placeholder: false,
      version: "0.13.0",
    });
    const staleFile = join(outputDirectory, "stale.txt");
    await writeFile(staleFile, "stale\n");

    await packageRelease({
      assetsDirectory,
      outputDirectory,
      placeholder: false,
      version: "0.13.0",
    });

    await Promise.resolve(
      expect(Bun.file(staleFile).exists()).resolves.toBe(false),
    );
    await verifyPackages(outputDirectory);
  });

  test("publication refuses a locally modified tarball before registry access", async () => {
    const outputDirectory = join(temporaryDirectory, "tampered");
    const manifest = await packageRelease({
      assetsDirectory,
      outputDirectory,
      placeholder: false,
      version: "0.13.0",
    });
    await appendFile(
      join(outputDirectory, "tarballs", manifest.packages[0]!.filename),
      "tampered",
    );
    let registryCalls = 0;

    await Promise.resolve(
      expect(
        publishPackages(outputDirectory, async () => {
          registryCalls += 1;
          return { json: async () => ({}), status: 404 };
        }),
      ).rejects.toThrow("size"),
    );
    expect(registryCalls).toBe(0);
  });

  test("stages inert placeholder packages without executables", async () => {
    const outputDirectory = join(temporaryDirectory, "placeholders");
    const manifest = await packageRelease({
      assetsDirectory,
      outputDirectory,
      placeholder: true,
      version: "0.0.0",
    });

    expect(manifest.packages).toHaveLength(6);
    expect(manifest.packages.some(({ name }) => name === "treeboot")).toBe(
      false,
    );
    await verifyPackages(outputDirectory);
    const firstTarball = await readFile(
      join(outputDirectory, "tarballs", manifest.packages[0]!.filename),
    );
    expect(firstTarball.length).toBeGreaterThan(0);
  });

  test("rejects a non-placeholder platform package without its binary", async () => {
    const outputDirectory = join(temporaryDirectory, "missing-binary");
    const manifest = await packageRelease({
      assetsDirectory,
      outputDirectory,
      placeholder: true,
      version: "0.0.0",
    });
    const artifact = manifest.packages[0]!;
    const tarball = await readFile(
      join(outputDirectory, "tarballs", artifact.filename),
    );
    const entries = readTarGzip(tarball).map((entry) => {
      if (entry.name !== "package/package.json") {
        return entry;
      }
      const packageJson = JSON.parse(entry.bytes.toString("utf8")) as Record<
        string,
        unknown
      >;
      packageJson.version = "0.13.0";
      return {
        ...entry,
        bytes: Buffer.from(`${JSON.stringify(packageJson)}\n`),
      };
    });

    expect(() => verifyTarball(artifact.name, "0.13.0", entries, true)).toThrow(
      "missing packaged executable",
    );
  });
});

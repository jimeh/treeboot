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
import { verifyPackages } from "../scripts/verify-package.ts";
import { publishPackages } from "../scripts/publish-packages.ts";

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
  const build = Bun.spawn([process.execPath, "run", "npm:build"], {
    stderr: "inherit",
    stdout: "inherit",
  });
  expect(await build.exited).toBe(0);
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

    expect(
      packageRelease({
        assetsDirectory,
        outputDirectory,
        placeholder: false,
        version: "0.13.0",
      }),
    ).rejects.toThrow("not owned by the Treeboot npm packager");
    expect(await readFile(sentinel, "utf8")).toBe("keep me\n");
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

    expect(Bun.file(staleFile).exists()).resolves.toBe(false);
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

    expect(
      publishPackages(outputDirectory, async () => {
        registryCalls += 1;
        return { json: async () => ({}), status: 404 };
      }),
    ).rejects.toThrow("size");
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
});

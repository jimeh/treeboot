import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import type { PackageArtifact, PackageManifest } from "./manifest.ts";
import { facadePackageName, platformPackages } from "./packages.ts";
import { verifyPackages } from "./verify-package.ts";

interface RegistryResponse {
  readonly status: number;
  json(): Promise<unknown>;
}

export type RegistryFetch = (url: string) => Promise<RegistryResponse>;
export type PublicationDecision = "publish" | "skip";

const root = fileURLToPath(new URL("../..", import.meta.url));
const registry = "https://registry.npmjs.org";

if (import.meta.main) {
  const outputDirectory = process.argv[2] ?? "npm-dist";
  await publishPackages(outputDirectory);
}

export async function decidePublication(
  artifact: PackageArtifact,
  registryFetch: RegistryFetch,
): Promise<PublicationDecision> {
  const response = await registryFetch(registryUrl(artifact));
  if (response.status === 404) {
    return "publish";
  }
  if (response.status !== 200) {
    throw new Error(
      `npm registry returned ${response.status} for ${artifact.name}@${artifact.version}`,
    );
  }
  const metadata = (await response.json()) as {
    dist?: { integrity?: unknown };
  };
  const publishedIntegrity = metadata.dist?.integrity;
  if (publishedIntegrity !== artifact.integrity) {
    throw new Error(
      `npm already has ${artifact.name}@${artifact.version} with integrity ` +
        `${String(publishedIntegrity)}, expected ${artifact.integrity}`,
    );
  }
  return "skip";
}

export async function publishPackages(
  outputDirectory: string,
  registryFetch: RegistryFetch = fetch,
): Promise<void> {
  await verifyPackages(outputDirectory);
  const absoluteOutput = resolve(root, outputDirectory);
  const manifest = JSON.parse(
    await readFile(join(absoluteOutput, "manifest.json"), "utf8"),
  ) as PackageManifest;
  assertPublicationOrder(manifest);

  for (const artifact of manifest.packages) {
    const decision = await decidePublication(artifact, registryFetch);
    if (decision === "skip") {
      console.log(
        `${artifact.name}@${artifact.version} already has the expected integrity; skipping`,
      );
      continue;
    }
    const tarball = join(absoluteOutput, "tarballs", artifact.filename);
    const child = Bun.spawn(
      ["npm", "publish", tarball, "--access", "public", "--provenance"],
      {
        stderr: "inherit",
        stdout: "inherit",
      },
    );
    const status = await child.exited;
    if (status !== 0) {
      throw new Error(
        `npm publish failed for ${artifact.name}@${artifact.version} with status ${status}`,
      );
    }
    await waitForPublication(artifact, registryFetch);
  }
}

async function waitForPublication(
  artifact: PackageArtifact,
  registryFetch: RegistryFetch,
): Promise<void> {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    const response = await registryFetch(registryUrl(artifact));
    if (response.status === 200) {
      const metadata = (await response.json()) as {
        dist?: { integrity?: unknown };
      };
      if (metadata.dist?.integrity === artifact.integrity) {
        return;
      }
      throw new Error(
        `npm registry returned unexpected integrity for ${artifact.name}@${artifact.version}`,
      );
    }
    if (response.status !== 404) {
      throw new Error(
        `npm registry returned ${response.status} while waiting for ` +
          `${artifact.name}@${artifact.version}`,
      );
    }
    await Bun.sleep(10_000);
  }
  throw new Error(
    `${artifact.name}@${artifact.version} did not appear in the npm registry in time`,
  );
}

function assertPublicationOrder(manifest: PackageManifest): void {
  const expected = [
    ...platformPackages.map(({ npmName }) => npmName),
    facadePackageName,
  ];
  const actual = manifest.packages.map(({ name }) => name);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `Refusing npm publication with unsafe order: ${actual.join(", ")}`,
    );
  }
}

function registryUrl(artifact: PackageArtifact): string {
  return `${registry}/${encodeURIComponent(artifact.name)}/${encodeURIComponent(artifact.version)}`;
}

import { describe, expect, test } from "bun:test";

import type { PackageArtifact } from "../scripts/manifest.ts";
import {
  decidePublication,
  type RegistryFetch,
} from "../scripts/publish-packages.ts";

const artifact: PackageArtifact = {
  filename: "treeboot-1.2.3.tgz",
  integrity: "sha512-expected",
  name: "treeboot",
  size: 123,
  version: "1.2.3",
};

describe("npm publication reruns", () => {
  test("publishes an absent package version", async () => {
    expect(await decidePublication(artifact, response(404))).toBe("publish");
  });

  test("skips an existing package with identical bytes", async () => {
    expect(
      await decidePublication(
        artifact,
        response(200, { dist: { integrity: artifact.integrity } }),
      ),
    ).toBe("skip");
  });

  test("rejects an existing package with different bytes", async () => {
    expect(
      decidePublication(
        artifact,
        response(200, { dist: { integrity: "sha512-different" } }),
      ),
    ).rejects.toThrow("with integrity sha512-different");
  });

  test("encodes a scoped package name as one registry path segment", async () => {
    const scopedArtifact = {
      ...artifact,
      name: "@treeboot-rs/linux-x64",
    };
    let requestedUrl = "";

    await decidePublication(scopedArtifact, async (url) => {
      requestedUrl = url;
      return { json: async () => ({}), status: 404 };
    });

    expect(requestedUrl).toBe(
      "https://registry.npmjs.org/%40treeboot-rs%2Flinux-x64/1.2.3",
    );
  });
});

function response(status: number, body: unknown = {}): RegistryFetch {
  return async () => ({
    json: async () => body,
    status,
  });
}

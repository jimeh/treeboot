import { chmod, copyFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../..", import.meta.url));
const source = join(root, "npm", "treeboot", "src");
const output = join(root, "npm", "treeboot", "dist");
const external = [
  "@treeboot-rs/darwin-arm64",
  "@treeboot-rs/darwin-x64",
  "@treeboot-rs/linux-arm64",
  "@treeboot-rs/linux-x64",
  "@treeboot-rs/win32-arm64",
  "@treeboot-rs/win32-x64",
];

await rm(output, { force: true, recursive: true });

await build({
  entrypoints: [join(source, "index.ts")],
  format: "esm",
  naming: "index.js",
});
await build({
  entrypoints: [join(source, "index.cjs.ts")],
  format: "cjs",
  naming: "index.cjs",
});
await build({
  entrypoints: [join(source, "bin.ts")],
  format: "esm",
  naming: "cli.js",
});

const declarations = Bun.spawn(
  [
    process.execPath,
    "x",
    "tsc",
    "--project",
    join(root, "npm", "treeboot", "tsconfig.build.json"),
  ],
  { cwd: root, stderr: "inherit", stdout: "inherit" },
);
const declarationStatus = await declarations.exited;
if (declarationStatus !== 0) {
  throw new Error(
    `TypeScript declaration build failed with ${declarationStatus}`,
  );
}

await copyFile(
  join(output, "types", "index.d.ts"),
  join(output, "types", "index.d.cts"),
);

await chmod(join(output, "cli.js"), 0o755);

async function build(
  options: Pick<Bun.BuildConfig, "entrypoints" | "format" | "naming">,
): Promise<void> {
  const result = await Bun.build({
    ...options,
    external,
    minify: false,
    outdir: output,
    packages: "external",
    sourcemap: "none",
    target: "node",
  });
  if (!result.success) {
    for (const log of result.logs) {
      console.error(log);
    }
    throw new Error(`Bun ${options.format} build failed`);
  }
}

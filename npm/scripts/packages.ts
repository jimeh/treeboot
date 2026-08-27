export interface PlatformDefinition {
  readonly arch: "arm64" | "x64";
  readonly assetName: string;
  readonly executableName: "treeboot" | "treeboot.exe";
  readonly npmName: `@treeboot-rs/${string}`;
  readonly os: "darwin" | "linux" | "win32";
  readonly workspace: string;
}

export const platformPackages: readonly PlatformDefinition[] = [
  {
    arch: "arm64",
    assetName: "treeboot-aarch64-apple-darwin",
    executableName: "treeboot",
    npmName: "@treeboot-rs/darwin-arm64",
    os: "darwin",
    workspace: "darwin-arm64",
  },
  {
    arch: "x64",
    assetName: "treeboot-x86_64-apple-darwin",
    executableName: "treeboot",
    npmName: "@treeboot-rs/darwin-x64",
    os: "darwin",
    workspace: "darwin-x64",
  },
  {
    arch: "arm64",
    assetName: "treeboot-aarch64-unknown-linux-musl",
    executableName: "treeboot",
    npmName: "@treeboot-rs/linux-arm64",
    os: "linux",
    workspace: "linux-arm64",
  },
  {
    arch: "x64",
    assetName: "treeboot-x86_64-unknown-linux-musl",
    executableName: "treeboot",
    npmName: "@treeboot-rs/linux-x64",
    os: "linux",
    workspace: "linux-x64",
  },
  {
    arch: "arm64",
    assetName: "treeboot-aarch64-pc-windows-msvc.exe",
    executableName: "treeboot.exe",
    npmName: "@treeboot-rs/win32-arm64",
    os: "win32",
    workspace: "win32-arm64",
  },
  {
    arch: "x64",
    assetName: "treeboot-x86_64-pc-windows-msvc.exe",
    executableName: "treeboot.exe",
    npmName: "@treeboot-rs/win32-x64",
    os: "win32",
    workspace: "win32-x64",
  },
];

export const facadePackageName = "treeboot";

export function packageFilename(name: string, version: string): string {
  return `${name.replace(/^@/, "").replaceAll("/", "-")}-${version}.tgz`;
}

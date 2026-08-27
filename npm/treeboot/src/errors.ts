/** Base class for errors raised while locating the packaged Treeboot binary. */
export class TreebootBinaryError extends Error {
  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = new.target.name;
  }
}

/** The current Node.js platform and architecture have no Treeboot package. */
export class UnsupportedPlatformError extends TreebootBinaryError {
  readonly platform: NodeJS.Platform;
  readonly arch: string;

  constructor(platform: NodeJS.Platform, arch: string) {
    super(
      `Treeboot does not publish an npm binary for ${platform}/${arch}. ` +
        "Supported targets are macOS, Linux, and Windows on arm64 or x64.",
    );
    this.platform = platform;
    this.arch = arch;
  }
}

/** The matching optional package or its executable is missing. */
export class MissingPlatformPackageError extends TreebootBinaryError {
  readonly packageName: string;
  readonly platform: NodeJS.Platform;
  readonly arch: string;

  constructor(
    packageName: string,
    platform: NodeJS.Platform,
    arch: string,
    cause?: unknown,
  ) {
    super(
      `Treeboot expected ${packageName} for ${platform}/${arch}, but its ` +
        "executable is unavailable. Reinstall with optional dependencies " +
        "enabled and make sure your bundler or Electron packager retains " +
        "the platform package and unpacks its bin directory from ASAR.",
      cause === undefined ? undefined : { cause },
    );
    this.packageName = packageName;
    this.platform = platform;
    this.arch = arch;
  }
}

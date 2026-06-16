# Release Harness

Release automation is intentionally pending. Use this guide when implementing
the release milestone from the spec.

## Release Contract

The spec expects:

- archive assets for each supported target
- raw executable assets for direct installers
- `treeboot-checksums.txt`
- detached signature for the checksum manifest
- SPDX SBOM
- SLSA/in-toto provenance where practical
- signed and notarized macOS binaries

## Future Tasks

Before the first real release, add or document commands for:

- building each supported target
- smoke-testing each built binary
- creating archives with `treeboot`, `README.md`, and `LICENSE`
- generating checksums
- generating SBOM/provenance artifacts
- signing checksums
- signing/notarizing macOS binaries

## Validation Expectations

Release work should run:

```sh
mise run verify
```

Release-specific automation should also have at least one local dry-run or smoke
command that does not publish anything.

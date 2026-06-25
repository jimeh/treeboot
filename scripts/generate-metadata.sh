#!/usr/bin/env bash
set -euo pipefail

mode="${1:-write}"

spec_version="$(
  sed -nE 's/.*Specification v([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' \
    docs/SPEC.md |
    head -n 1
)"

if [[ -z "${spec_version}" ]]; then
  printf 'failed to determine spec version from docs/SPEC.md\n' >&2
  exit 1
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

cat >"$tmp" <<EOF
//! Generated treeboot metadata.
//!
//! Regenerate with \`mise run generate:metadata\`.

use serde::Serialize;

/// treeboot spec version implemented by this crate.
pub const SPEC_VERSION: &str = "${spec_version}";

/// Bundled treeboot config JSON Schema.
pub const CONFIG_SCHEMA_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/config.schema.json"
));

/// Returns the bundled treeboot config JSON Schema.
#[must_use]
pub const fn config_schema_json() -> &'static str {
    CONFIG_SCHEMA_JSON
}

/// treeboot version metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VersionInfo {
    /// Package name.
    pub package: &'static str,
    /// Package version.
    pub version: &'static str,
    /// Implemented treeboot spec version.
    pub spec_version: &'static str,
}

/// Returns version metadata for a package implementing treeboot.
#[must_use]
pub const fn version_info(package: &'static str, version: &'static str) -> VersionInfo {
    VersionInfo {
        package,
        version,
        spec_version: SPEC_VERSION,
    }
}
EOF

target="crates/treeboot-core/src/metadata.rs"

case "${mode}" in
  write)
    cp "$tmp" "$target"
    ;;
  check)
    diff -u "$target" "$tmp"
    ;;
  *)
    printf 'usage: %s [write|check]\n' "$0" >&2
    exit 2
    ;;
esac

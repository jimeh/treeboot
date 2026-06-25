//! Generated treeboot metadata.
//!
//! Regenerate with `mise run generate:metadata`.

use serde::Serialize;
use std::sync::OnceLock;

/// treeboot spec version implemented by this crate.
pub const SPEC_VERSION: &str = "1.9.0";

/// treeboot package name used for product-level version reporting.
pub const TREEBOOT_PACKAGE: &str = "treeboot";

/// treeboot package version.
///
/// `treeboot` and `treeboot-core` package versions are intentionally released
/// in lockstep.
pub const TREEBOOT_VERSION: &str = env!("CARGO_PKG_VERSION");

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

/// Returns product-level treeboot version metadata.
#[must_use]
pub const fn treeboot_version_info() -> VersionInfo {
    version_info(TREEBOOT_PACKAGE, TREEBOOT_VERSION)
}

/// Returns the treeboot version summary used by CLI version flags.
#[must_use]
pub fn treeboot_version_summary() -> &'static str {
    static SUMMARY: OnceLock<String> = OnceLock::new();

    SUMMARY.get_or_init(|| format!("{TREEBOOT_VERSION} (spec {SPEC_VERSION})"))
}

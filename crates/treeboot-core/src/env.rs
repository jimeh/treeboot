use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::context;
use crate::{Config, EnvironmentInput, Result, WorktreeOptions};

/// Options for inspecting the treeboot child environment.
///
/// Construct options through [`EnvOptions::default`] so future additive fields
/// remain source-compatible.
///
/// ```
/// use treeboot_core::EnvOptions;
///
/// let mut options = EnvOptions::default();
/// options.config = Some(".treeboot.toml".into());
/// ```
///
/// ```compile_fail
/// use treeboot_core::{EnvOptions, EnvironmentInput};
///
/// let _ = EnvOptions {
///     cwd: None,
///     root: None,
///     environment: EnvironmentInput::empty(),
///     config: None,
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EnvOptions {
    /// Directory from which environment discovery starts.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used for discovery.
    pub root: Option<PathBuf>,
    /// Explicit environment input used for compatibility discovery.
    pub environment: EnvironmentInput,
    /// Uses one specific config file instead of config discovery.
    pub config: Option<PathBuf>,
}

/// Result summary for a `treeboot env` invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EnvReport {
    /// Environment variables passed to configured commands.
    pub environment: BTreeMap<String, String>,
}

/// Inspects the treeboot child environment.
///
/// This function loads config only to resolve the effective identifier. It does
/// not apply file operations or execute commands.
///
/// # Errors
///
/// Returns an error if context or config discovery, loading, or parsing fails.
pub fn inspect_env(options: EnvOptions) -> Result<EnvReport> {
    let context = context::resolve(&WorktreeOptions {
        cwd: options.cwd,
        root: options.root,
        environment: options.environment,
    })?;
    let context = Config::load_discovered(&context, options.config.as_deref())?
        .map_or(context, |loaded| loaded.context);
    let environment = context
        .environment
        .into_iter()
        .map(|(name, value)| (name, value.to_string_lossy().into_owned()))
        .collect();

    Ok(EnvReport { environment })
}

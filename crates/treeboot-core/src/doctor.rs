use std::path::PathBuf;

use serde::Serialize;

use crate::check::WorktreeSnapshot;
use crate::config::RuntimeOptionOverrides;
use crate::context;
use crate::{ActionPlan, Config, EnvironmentInput, InitScriptDiscovery, WorktreeOptions};

/// Options for diagnosing treeboot discovery and validation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DoctorOptions {
    /// Directory from which diagnostics start. Defaults to the process cwd.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used as the file-operation source.
    pub root: Option<PathBuf>,
    /// Explicit environment input used for compatibility discovery and options.
    pub environment: EnvironmentInput,
    /// Uses one specific config file and skips init script discovery.
    pub config: Option<PathBuf>,
    /// Skips init script discovery and uses declarative config discovery.
    pub no_init_script: bool,
}

/// Diagnostic status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStatus {
    /// The check passed.
    Ok,
    /// The check found a non-fatal issue.
    Warning,
    /// The check found a fatal issue.
    Error,
}

/// One doctor diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// Stable diagnostic name.
    pub name: &'static str,
    /// Diagnostic status.
    pub status: DiagnosticStatus,
    /// Human-readable diagnostic message.
    pub message: String,
}

/// Result summary for a `treeboot doctor` invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorReport {
    /// Whether any diagnostic is fatal.
    pub fatal: bool,
    /// Discovered worktree context, when available.
    pub context: Option<WorktreeSnapshot>,
    /// Ordered diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

impl DoctorReport {
    /// Returns true when the report contains any fatal diagnostic.
    #[must_use]
    pub fn has_fatal(&self) -> bool {
        self.fatal
    }
}

/// Diagnoses treeboot discovery and validation without side effects.
#[must_use]
pub fn diagnose(options: DoctorOptions) -> DoctorReport {
    let mut diagnostics = Vec::new();
    let mut fatal = false;

    let env_options = match RuntimeOptionOverrides::from_environment(&options.environment) {
        Ok(options) => {
            diagnostics.push(ok("environment_options", "environment options are valid"));
            options
        }
        Err(error) => {
            diagnostics.push(error_diag("environment_options", error.to_string()));
            return DoctorReport {
                fatal: true,
                context: None,
                diagnostics,
            };
        }
    };

    let context = match context::resolve(&WorktreeOptions {
        cwd: options.cwd.clone(),
        root: options.root.clone(),
        environment: options.environment.clone(),
    }) {
        Ok(context) => {
            diagnostics.push(ok("worktree", "worktree context resolved"));
            diagnostics.push(ok("root", "root checkout resolved"));
            if context.default_branch.is_empty() {
                diagnostics.push(warning("default_branch", "default branch unknown"));
            } else {
                diagnostics.push(ok("default_branch", "default branch resolved"));
            }
            diagnostics.push(ok("environment", "child environment built"));
            context
        }
        Err(error) => {
            diagnostics.push(error_diag("worktree", error.to_string()));
            return DoctorReport {
                fatal: true,
                context: None,
                diagnostics,
            };
        }
    };
    let context_snapshot = WorktreeSnapshot::from(&context);

    if !options.no_init_script && options.config.is_none() {
        let scripts = InitScriptDiscovery::discover(&context);
        if let Some(path) = scripts.executable {
            diagnostics.push(ok(
                "init_script",
                format!("executable init script found: {}", path.display()),
            ));
        } else if scripts.ignored.is_empty() {
            diagnostics.push(warning("init_script", "no executable init script found"));
        } else {
            diagnostics.push(warning(
                "init_script",
                format!(
                    "no executable init script found; ignored {} non-executable path(s)",
                    scripts.ignored.len()
                ),
            ));
        }
    } else {
        diagnostics.push(ok("init_script", "init script discovery skipped"));
    }

    match check_config(&options, &context, env_options) {
        Ok(diagnostic) => diagnostics.push(diagnostic),
        Err(diagnostic) => {
            fatal = true;
            diagnostics.push(diagnostic);
        }
    }

    DoctorReport {
        fatal,
        context: Some(context_snapshot),
        diagnostics,
    }
}

fn check_config(
    options: &DoctorOptions,
    context: &crate::Worktree,
    env_options: RuntimeOptionOverrides,
) -> std::result::Result<Diagnostic, Diagnostic> {
    let path = Config::discover_path(context, options.config.as_deref())
        .map_err(|error| error_diag("config", error.to_string()))?;

    let Some(path) = path else {
        return Ok(warning("config", "no config detected"));
    };

    let config =
        Config::load(&path, context).map_err(|error| error_diag("config", error.to_string()))?;
    let plan_options = env_options.resolve(&config.options, false);
    ActionPlan::from_manifest(&path, &config, context, plan_options.into())
        .map_err(|error| error_diag("config_validation", error.to_string()))?;

    Ok(ok("config", format!("config is valid: {}", path.display())))
}

fn ok(name: &'static str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        name,
        status: DiagnosticStatus::Ok,
        message: message.into(),
    }
}

fn warning(name: &'static str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        name,
        status: DiagnosticStatus::Warning,
        message: message.into(),
    }
}

fn error_diag(name: &'static str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        name,
        status: DiagnosticStatus::Error,
        message: message.into(),
    }
}

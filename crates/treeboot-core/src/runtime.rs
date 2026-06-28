use crate::{ActionPlanOptions, ConfigRuntimeOptions, EnvironmentInput, Error, Result};

/// Environment overrides for config runtime options.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuntimeOptionOverrides {
    /// Strict mode environment override.
    pub strict: Option<bool>,
    /// Source-boundary environment override.
    pub dangerously_allow_sources_outside_root: Option<bool>,
    /// Target-boundary environment override.
    pub dangerously_allow_targets_outside_worktree: Option<bool>,
}

impl RuntimeOptionOverrides {
    /// Parses treeboot runtime option overrides from explicit environment input.
    ///
    /// # Errors
    ///
    /// Returns an error when an environment value is not a supported boolean.
    pub fn from_environment(environment: &EnvironmentInput) -> Result<Self> {
        Ok(Self {
            strict: env_bool("TREEBOOT_STRICT", environment.treeboot_strict.as_deref())?,
            dangerously_allow_sources_outside_root: env_bool(
                "TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT",
                environment
                    .treeboot_dangerously_allow_sources_outside_root
                    .as_deref(),
            )?,
            dangerously_allow_targets_outside_worktree: env_bool(
                "TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE",
                environment
                    .treeboot_dangerously_allow_targets_outside_worktree
                    .as_deref(),
            )?,
        })
    }

    /// Reads treeboot runtime option overrides from the process environment.
    ///
    /// # Errors
    ///
    /// Returns an error when an environment value is not a supported boolean.
    pub fn from_process_env() -> Result<Self> {
        Self::from_environment(&EnvironmentInput::from_process_env())
    }
}

/// Runtime policy resolved from environment and CLI input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimePolicy {
    overrides: RuntimeOptionOverrides,
    cli_strict: bool,
}

impl RuntimePolicy {
    /// Parses runtime policy from explicit environment input and CLI strictness.
    ///
    /// # Errors
    ///
    /// Returns an error when an environment value is not a supported boolean.
    pub fn from_environment(environment: &EnvironmentInput, cli_strict: bool) -> Result<Self> {
        Ok(Self::from_overrides(
            RuntimeOptionOverrides::from_environment(environment)?,
            cli_strict,
        ))
    }

    /// Reads runtime policy from the process environment and CLI strictness.
    ///
    /// # Errors
    ///
    /// Returns an error when an environment value is not a supported boolean.
    pub fn from_process_env(cli_strict: bool) -> Result<Self> {
        Ok(Self::from_overrides(
            RuntimeOptionOverrides::from_process_env()?,
            cli_strict,
        ))
    }

    /// Builds runtime policy from parsed environment overrides.
    #[must_use]
    pub const fn from_overrides(overrides: RuntimeOptionOverrides, cli_strict: bool) -> Self {
        Self {
            overrides,
            cli_strict,
        }
    }

    /// Returns strict mode before config discovery.
    #[must_use]
    pub const fn pre_config_strict(&self) -> bool {
        self.cli_strict || matches!(self.overrides.strict, Some(true))
    }

    /// Resolves runtime options using defaults, config, environment, then CLI.
    #[must_use]
    pub fn resolve(&self, config: &ConfigRuntimeOptions) -> ResolvedRuntimePolicy {
        let mut options = config.clone();

        if let Some(strict) = self.overrides.strict {
            options.strict = strict;
        }
        if let Some(allow) = self.overrides.dangerously_allow_sources_outside_root {
            options.dangerously_allow_sources_outside_root = allow;
        }
        if let Some(allow) = self.overrides.dangerously_allow_targets_outside_worktree {
            options.dangerously_allow_targets_outside_worktree = allow;
        }
        if self.cli_strict {
            options.strict = true;
        }

        ResolvedRuntimePolicy { options }
    }
}

/// Runtime policy after config defaults, environment, and CLI are merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRuntimePolicy {
    options: ConfigRuntimeOptions,
}

impl ResolvedRuntimePolicy {
    /// Returns the resolved config-compatible runtime options.
    #[must_use]
    pub const fn options(&self) -> &ConfigRuntimeOptions {
        &self.options
    }

    /// Consumes the policy and returns the resolved config-compatible options.
    #[must_use]
    pub fn into_options(self) -> ConfigRuntimeOptions {
        self.options
    }

    /// Returns whether strict mode is enabled.
    #[must_use]
    pub const fn strict(&self) -> bool {
        self.options.strict
    }

    /// Returns default ignore patterns from the resolved policy.
    #[must_use]
    pub fn default_ignore(&self) -> &[String] {
        &self.options.default_ignore
    }

    /// Returns action-plan validation options for this resolved policy.
    #[must_use]
    pub fn action_plan_options(&self) -> ActionPlanOptions {
        ActionPlanOptions::from(self.options.clone())
    }
}

fn env_bool(name: &'static str, value: Option<&std::ffi::OsStr>) -> Result<Option<bool>> {
    let Some(value) = value else {
        return Ok(None);
    };

    let Some(value) = value.to_str() else {
        return Err(Error::InvalidBooleanEnv {
            name,
            value: value.to_string_lossy().into_owned(),
        });
    };

    parse_bool(value)
        .ok_or_else(|| Error::InvalidBooleanEnv {
            name,
            value: value.to_owned(),
        })
        .map(Some)
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    #[test]
    fn runtime_option_overrides_should_parse_explicit_environment_input() {
        let overrides = RuntimeOptionOverrides::from_environment(&EnvironmentInput {
            treeboot_strict: Some(OsString::from("yes")),
            treeboot_dangerously_allow_sources_outside_root: Some(OsString::from("true")),
            treeboot_dangerously_allow_targets_outside_worktree: Some(OsString::from("0")),
            ..EnvironmentInput::empty()
        })
        .expect("environment should parse");

        assert_eq!(
            overrides,
            RuntimeOptionOverrides {
                strict: Some(true),
                dangerously_allow_sources_outside_root: Some(true),
                dangerously_allow_targets_outside_worktree: Some(false),
            }
        );
    }

    #[test]
    fn runtime_option_overrides_should_reject_invalid_explicit_environment_input() {
        let error = RuntimeOptionOverrides::from_environment(&EnvironmentInput {
            treeboot_strict: Some(OsString::from("sometimes")),
            ..EnvironmentInput::empty()
        })
        .expect_err("environment should fail");

        assert!(matches!(
            error,
            Error::InvalidBooleanEnv {
                name: "TREEBOOT_STRICT",
                ..
            }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn runtime_option_overrides_should_reject_non_utf8_explicit_environment_input() {
        use std::os::unix::ffi::OsStringExt;

        let error = RuntimeOptionOverrides::from_environment(&EnvironmentInput {
            treeboot_strict: Some(OsString::from_vec(vec![0xFF])),
            ..EnvironmentInput::empty()
        })
        .expect_err("environment should fail");

        assert!(matches!(
            error,
            Error::InvalidBooleanEnv {
                name: "TREEBOOT_STRICT",
                ..
            }
        ));
    }

    #[test]
    fn runtime_policy_should_resolve_config_defaults() {
        let policy = RuntimePolicy::from_overrides(RuntimeOptionOverrides::default(), false);
        let config = ConfigRuntimeOptions {
            strict: true,
            default_ignore: vec![".DS_Store".to_owned()],
            dangerously_allow_sources_outside_root: true,
            dangerously_allow_targets_outside_worktree: false,
        };

        let resolved = policy.resolve(&config);

        assert_eq!(resolved.options(), &config);
    }

    #[test]
    fn runtime_policy_should_apply_environment_overrides_after_config() {
        let policy = RuntimePolicy::from_overrides(
            RuntimeOptionOverrides {
                strict: Some(false),
                dangerously_allow_sources_outside_root: Some(false),
                dangerously_allow_targets_outside_worktree: Some(true),
            },
            false,
        );
        let config = ConfigRuntimeOptions {
            strict: true,
            default_ignore: vec!["build".to_owned()],
            dangerously_allow_sources_outside_root: true,
            dangerously_allow_targets_outside_worktree: false,
        };

        let resolved = policy.resolve(&config);

        assert_eq!(
            resolved.into_options(),
            ConfigRuntimeOptions {
                strict: false,
                default_ignore: vec!["build".to_owned()],
                dangerously_allow_sources_outside_root: false,
                dangerously_allow_targets_outside_worktree: true,
            }
        );
    }

    #[test]
    fn runtime_policy_should_apply_cli_strict_after_environment() {
        let policy = RuntimePolicy::from_overrides(
            RuntimeOptionOverrides {
                strict: Some(false),
                ..RuntimeOptionOverrides::default()
            },
            true,
        );

        let resolved = policy.resolve(&ConfigRuntimeOptions::default());

        assert!(resolved.strict());
    }

    #[test]
    fn runtime_policy_should_enable_pre_config_strict_from_cli_or_environment() {
        let env_policy = RuntimePolicy::from_overrides(
            RuntimeOptionOverrides {
                strict: Some(true),
                ..RuntimeOptionOverrides::default()
            },
            false,
        );
        let cli_policy = RuntimePolicy::from_overrides(RuntimeOptionOverrides::default(), true);

        assert!(env_policy.pre_config_strict());
        assert!(cli_policy.pre_config_strict());
    }

    #[test]
    fn parse_bool_should_accept_supported_true_values() {
        for value in ["1", "true", "TRUE", "yes", "on"] {
            assert_eq!(parse_bool(value), Some(true), "value {value:?}");
        }
    }

    #[test]
    fn parse_bool_should_accept_supported_false_values() {
        for value in ["0", "false", "FALSE", "no", "off"] {
            assert_eq!(parse_bool(value), Some(false), "value {value:?}");
        }
    }

    #[test]
    fn parse_bool_should_reject_unsupported_values() {
        assert_eq!(parse_bool("sometimes"), None);
    }
}

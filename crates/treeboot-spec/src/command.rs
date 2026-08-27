use std::ffi::{OsStr, OsString};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

use serde::Serialize;

/// A candidate program and the arguments placed before every case invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandTemplate {
    program: OsString,
    prefix_args: Vec<OsString>,
}

impl CommandTemplate {
    /// Creates a template that invokes `program` directly, without a shell.
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            prefix_args: Vec::new(),
        }
    }

    /// Creates a template with native prefix arguments.
    pub fn with_args<I, S>(program: impl Into<OsString>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        Self {
            program: program.into(),
            prefix_args: args.into_iter().map(Into::into).collect(),
        }
    }

    /// Appends one prefix argument and returns the updated template.
    #[must_use]
    pub fn with_prefix_arg(mut self, arg: impl Into<OsString>) -> Self {
        self.prefix_args.push(arg.into());
        self
    }

    /// Returns the candidate program.
    pub fn program(&self) -> &OsStr {
        &self.program
    }

    /// Returns the arguments placed before each case's arguments.
    pub fn prefix_args(&self) -> &[OsString] {
        &self.prefix_args
    }

    /// Returns a report-safe, lossy projection of the native command.
    pub fn report(&self) -> CommandReport {
        CommandReport {
            program: self.program.to_string_lossy().into_owned(),
            prefix_args: self
                .prefix_args
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect(),
        }
    }

    /// Resolves a path-like program before case working directories are entered.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the program cannot be found or made absolute.
    pub fn resolve(&self) -> std::io::Result<Self> {
        let path = Path::new(&self.program);
        let program = if path.components().count() == 1 {
            resolve_from_path(path)?
        } else {
            let path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()?.join(path)
            };
            dunce::canonicalize(path)?
        };
        Ok(Self {
            program: program.into_os_string(),
            prefix_args: self.prefix_args.clone(),
        })
    }
}

fn resolve_from_path(program: &Path) -> std::io::Result<PathBuf> {
    let search_path = std::env::var_os("PATH").ok_or_else(|| {
        Error::new(
            ErrorKind::NotFound,
            format!("cannot find {:?}: PATH is not set", program.as_os_str()),
        )
    })?;
    for directory in std::env::split_paths(&search_path) {
        for candidate in executable_candidates(directory.join(program)) {
            if candidate.is_file() {
                return dunce::canonicalize(candidate);
            }
        }
    }
    Err(Error::new(
        ErrorKind::NotFound,
        format!("cannot find {:?} in PATH", program.as_os_str()),
    ))
}

#[cfg(not(windows))]
fn executable_candidates(candidate: PathBuf) -> Vec<PathBuf> {
    vec![candidate]
}

#[cfg(windows)]
fn executable_candidates(candidate: PathBuf) -> Vec<PathBuf> {
    if candidate.extension().is_some() {
        return vec![candidate];
    }
    let extensions = std::env::var_os("PATHEXT")
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into())
        .to_string_lossy()
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(|extension| candidate.with_extension(extension.trim_start_matches('.')))
        .collect::<Vec<_>>();
    if extensions.is_empty() {
        vec![candidate]
    } else {
        extensions
    }
}

/// Serializable projection of a native candidate command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandReport {
    /// Candidate program rendered lossily for reports.
    pub program: String,
    /// Prefix arguments rendered lossily for reports.
    pub prefix_args: Vec<String>,
}

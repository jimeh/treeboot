use std::path::{Component, Path, PathBuf};

use globset::{GlobBuilder, GlobMatcher};

use crate::ignore_rules::PathIgnoreRules;

/// Returns whether a declared source path is a glob pattern.
///
/// Detection looks for the glob metacharacters `*`, `?`, or `[` in any
/// normal path component. Prefix and root components never trigger
/// detection.
pub(crate) fn is_glob_source(source: &Path) -> bool {
    source.components().any(|component| match component {
        Component::Normal(value) => component_has_metacharacters(value.as_encoded_bytes()),
        _ => false,
    })
}

fn component_has_metacharacters(value: &[u8]) -> bool {
    value.iter().any(|byte| matches!(byte, b'*' | b'?' | b'['))
}

/// A glob source split into its literal base and pattern components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SplitGlobSource {
    /// Declared literal base path, before the first pattern component.
    pub(crate) base: PathBuf,
    /// Pattern components after the base, in declaration order.
    pub(crate) components: Vec<String>,
}

/// Context-agnostic glob source failure, resolved to a public error at the
/// caller boundary.
#[derive(Debug)]
pub(crate) enum GlobSourceError {
    /// The pattern contains a `..` component after the base.
    UnsupportedComponent(&'static str),
    /// A pattern component is not valid UTF-8.
    NotUtf8,
    /// The pattern is not a valid glob.
    InvalidPattern(globset::Error),
    /// Walking the base directory failed.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for GlobSourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedComponent(component) => write!(
                formatter,
                "`{component}` components are not supported after glob pattern components"
            ),
            Self::NotUtf8 => formatter.write_str("glob pattern components must be valid UTF-8"),
            Self::InvalidPattern(source) => write!(formatter, "invalid glob pattern: {source}"),
            Self::Io { path, source } => write!(
                formatter,
                "failed to inspect glob source directory {}: {source}",
                path.display()
            ),
        }
    }
}

/// Splits a glob source into its literal base and pattern components.
///
/// The base is the longest leading run of path components without glob
/// metacharacters. `..` components are rejected after the first pattern
/// component; `.` components normalize away.
pub(crate) fn split_glob_source(source: &Path) -> Result<SplitGlobSource, GlobSourceError> {
    let mut base = PathBuf::new();
    let mut components = Vec::new();

    for component in source.components() {
        match component {
            Component::Normal(value) if components.is_empty() => {
                if component_has_metacharacters(value.as_encoded_bytes()) {
                    let value = value.to_str().ok_or(GlobSourceError::NotUtf8)?;
                    components.push(value.to_owned());
                } else {
                    base.push(value);
                }
            }
            Component::Normal(value) => {
                let value = value.to_str().ok_or(GlobSourceError::NotUtf8)?;
                components.push(value.to_owned());
            }
            // `Path::components` normalizes away non-leading `.` components,
            // so `.` never needs rejection after the pattern starts.
            Component::CurDir => {}
            Component::ParentDir if components.is_empty() => base.push(component),
            Component::ParentDir => return Err(GlobSourceError::UnsupportedComponent("..")),
            Component::Prefix(_) | Component::RootDir => base.push(component),
        }
    }

    Ok(SplitGlobSource { base, components })
}

/// A path matched by glob source expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GlobSourceMatch {
    /// Path of the match relative to the pattern base.
    pub(crate) relative: PathBuf,
    /// Whether the match is a directory. Symlinks are never directories.
    pub(crate) is_dir: bool,
}

/// Expands a split glob source against the filesystem under `base_path`.
///
/// Matches are ordered lexicographically by base-relative path. Matched
/// directories are not descended into, so descendant matches inside matched
/// directories are pruned. Directory symlinks are never followed. Ignore
/// rules anchored at the pattern base filter matches; ignored directories
/// are still traversed conservatively when negated patterns exist.
pub(crate) fn expand_glob_source(
    base_path: &Path,
    split: &SplitGlobSource,
    ignore: Option<&PathIgnoreRules>,
) -> Result<Vec<GlobSourceMatch>, GlobSourceError> {
    let matcher = build_matcher(&split.components)?;
    let max_depth = if split.components.iter().any(|component| component == "**") {
        None
    } else {
        Some(split.components.len())
    };
    let mut matches = Vec::new();

    walk(
        &WalkContext {
            matcher,
            max_depth,
            ignore,
        },
        base_path,
        Path::new(""),
        1,
        &mut matches,
    )?;
    matches.sort_by(|left, right| left.relative.cmp(&right.relative));

    Ok(matches)
}

struct WalkContext<'a> {
    matcher: GlobMatcher,
    max_depth: Option<usize>,
    ignore: Option<&'a PathIgnoreRules>,
}

fn build_matcher(components: &[String]) -> Result<GlobMatcher, GlobSourceError> {
    let pattern = components
        .iter()
        .map(|component| component.replace('{', "[{]").replace('}', "[}]"))
        .collect::<Vec<_>>()
        .join("/");

    GlobBuilder::new(&pattern)
        .literal_separator(true)
        .backslash_escape(false)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(GlobSourceError::InvalidPattern)
}

fn walk(
    context: &WalkContext<'_>,
    directory: &Path,
    relative: &Path,
    depth: usize,
    matches: &mut Vec<GlobSourceMatch>,
) -> Result<(), GlobSourceError> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(GlobSourceError::Io {
                path: directory.to_path_buf(),
                source,
            });
        }
    };

    for entry in entries {
        let entry = entry.map_err(|source| GlobSourceError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let entry_path = entry.path();
        let metadata =
            std::fs::symlink_metadata(&entry_path).map_err(|source| GlobSourceError::Io {
                path: entry_path.clone(),
                source,
            })?;
        let is_dir = metadata.is_dir();
        let entry_relative = relative.join(entry.file_name());
        let may_descend = is_dir && context.max_depth.is_none_or(|max_depth| depth < max_depth);

        if context
            .ignore
            .is_some_and(|rules| rules.is_ignored(&entry_relative, is_dir))
        {
            if may_descend
                && context
                    .ignore
                    .map(PathIgnoreRules::has_negation)
                    .unwrap_or(false)
            {
                walk(context, &entry_path, &entry_relative, depth + 1, matches)?;
            }
            continue;
        }

        if context.matcher.is_match(&entry_relative) {
            matches.push(GlobSourceMatch {
                relative: entry_relative,
                is_dir,
            });
            continue;
        }

        if may_descend {
            walk(context, &entry_path, &entry_relative, depth + 1, matches)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::test_support::symlink_dir;

    fn temp_base(name: &str) -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after Unix epoch")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("treeboot-glob-{name}-{id}"));
        std::fs::create_dir_all(&base).expect("base should be created");
        base
    }

    fn split(source: &str) -> SplitGlobSource {
        split_glob_source(Path::new(source)).expect("source should split")
    }

    fn expand(base: &Path, source: &str) -> Vec<String> {
        expand_with_ignore(base, source, &[])
    }

    fn expand_with_ignore(base: &Path, source: &str, ignore: &[&str]) -> Vec<String> {
        let split = split(source);
        let rules = if ignore.is_empty() {
            None
        } else {
            Some(
                PathIgnoreRules::new(
                    base,
                    &ignore.iter().map(ToString::to_string).collect::<Vec<_>>(),
                )
                .expect("ignore rules should build"),
            )
        };

        expand_glob_source(base, &split, rules.as_ref())
            .expect("expansion should succeed")
            .into_iter()
            .map(|entry| {
                entry
                    .relative
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/")
            })
            .collect()
    }

    #[test]
    fn is_glob_source_should_detect_metacharacters() {
        assert!(is_glob_source(Path::new("certs/*.pem")));
        assert!(is_glob_source(Path::new("file?.txt")));
        assert!(is_glob_source(Path::new("file[0-9].txt")));
        assert!(!is_glob_source(Path::new("certs/a.pem")));
        assert!(!is_glob_source(Path::new("plain/path")));
    }

    #[test]
    fn split_glob_source_should_use_longest_literal_prefix() {
        let split = split("traefik/certs/*.pem");

        assert_eq!(split.base, PathBuf::from("traefik/certs"));
        assert_eq!(split.components, vec!["*.pem".to_owned()]);
    }

    #[test]
    fn split_glob_source_should_keep_components_after_first_pattern() {
        let split = split("traefik/**/sub/*.pem");

        assert_eq!(split.base, PathBuf::from("traefik"));
        assert_eq!(
            split.components,
            vec!["**".to_owned(), "sub".to_owned(), "*.pem".to_owned()]
        );
    }

    #[test]
    fn split_glob_source_should_reject_parent_components_after_pattern() {
        let error = split_glob_source(Path::new("certs/*/../other")).expect_err("should fail");

        assert!(error.to_string().contains("`..`"));
    }

    #[test]
    fn expand_glob_source_should_match_direct_children_only() {
        let base = temp_base("direct-children");
        std::fs::write(base.join("a.pem"), "a").expect("file should be written");
        std::fs::write(base.join("b.pem"), "b").expect("file should be written");
        std::fs::write(base.join("c.txt"), "c").expect("file should be written");
        std::fs::create_dir_all(base.join("sub")).expect("dir should be created");
        std::fs::write(base.join("sub/d.pem"), "d").expect("file should be written");

        assert_eq!(expand(&base, "*.pem"), vec!["a.pem", "b.pem"]);
    }

    #[test]
    fn expand_glob_source_should_match_hidden_files() {
        let base = temp_base("hidden");
        std::fs::write(base.join(".env"), "x").expect("file should be written");

        assert_eq!(expand(&base, "*"), vec![".env"]);
    }

    #[test]
    fn expand_glob_source_should_match_recursively_with_globstar() {
        let base = temp_base("globstar");
        std::fs::create_dir_all(base.join("a/b")).expect("dirs should be created");
        std::fs::write(base.join("top.pem"), "x").expect("file should be written");
        std::fs::write(base.join("a/mid.pem"), "x").expect("file should be written");
        std::fs::write(base.join("a/b/deep.pem"), "x").expect("file should be written");

        assert_eq!(
            expand(&base, "**/*.pem"),
            vec!["a/b/deep.pem", "a/mid.pem", "top.pem"]
        );
    }

    #[test]
    fn expand_glob_source_should_prune_matches_inside_matched_directories() {
        let base = temp_base("prune");
        std::fs::create_dir_all(base.join("dir/nested")).expect("dirs should be created");
        std::fs::write(base.join("dir/nested/file"), "x").expect("file should be written");
        std::fs::write(base.join("file"), "x").expect("file should be written");

        assert_eq!(expand(&base, "**"), vec!["dir", "file"]);
    }

    #[test]
    fn expand_glob_source_should_not_follow_directory_symlinks() {
        let base = temp_base("symlinks");
        std::fs::create_dir_all(base.join("real")).expect("dir should be created");
        std::fs::write(base.join("real/inner.pem"), "x").expect("file should be written");
        symlink_dir(base.join("real"), base.join("linked")).expect("symlink should be created");

        assert_eq!(
            expand(&base, "**/*.pem"),
            vec!["real/inner.pem"],
            "matches through the symlinked directory should be absent"
        );
        assert_eq!(expand(&base, "link*"), vec!["linked"]);
    }

    #[test]
    fn expand_glob_source_should_return_empty_for_missing_base() {
        let base = temp_base("missing-base");

        assert!(expand(&base.join("missing"), "*.pem").is_empty());
    }

    #[test]
    fn expand_glob_source_should_drop_ignored_matches() {
        let base = temp_base("ignored");
        std::fs::write(base.join("keep.pem"), "x").expect("file should be written");
        std::fs::write(base.join("foo.pem"), "x").expect("file should be written");

        assert_eq!(
            expand_with_ignore(&base, "*.pem", &["foo.pem"]),
            vec!["keep.pem"]
        );
    }

    #[test]
    fn expand_glob_source_should_reinclude_negated_matches() {
        let base = temp_base("negated");
        std::fs::create_dir_all(base.join("vendor/keep")).expect("dirs should be created");
        std::fs::write(base.join("vendor/keep/a.pem"), "x").expect("file should be written");
        std::fs::write(base.join("vendor/drop.pem"), "x").expect("file should be written");

        assert_eq!(
            expand_with_ignore(&base, "**/*.pem", &["vendor/**", "!vendor/keep/**"]),
            vec!["vendor/keep/a.pem"]
        );
    }

    #[test]
    fn expand_glob_source_should_treat_braces_as_literals() {
        let base = temp_base("braces");
        std::fs::write(base.join("{a}.pem"), "x").expect("file should be written");
        std::fs::write(base.join("b.pem"), "x").expect("file should be written");

        assert_eq!(expand(&base, "{a}*"), vec!["{a}.pem"]);
    }

    #[test]
    fn expand_glob_source_should_match_literal_metacharacters_with_classes() {
        let base = temp_base("classes");
        std::fs::write(base.join("a.pem"), "x").expect("file should be written");

        assert_eq!(expand(&base, "[ab].pem"), vec!["a.pem"]);
    }

    #[test]
    fn expand_glob_source_should_match_single_character_wildcards() {
        let base = temp_base("question-mark");
        std::fs::write(base.join("a1.pem"), "x").expect("file should be written");
        std::fs::write(base.join("a12.pem"), "x").expect("file should be written");

        assert_eq!(expand(&base, "a?.pem"), vec!["a1.pem"]);
    }

    #[cfg(windows)]
    #[test]
    fn split_glob_source_should_treat_backslashes_as_separators_on_windows() {
        let split = split("traefik\\certs\\*.pem");

        assert_eq!(split.base, PathBuf::from("traefik").join("certs"));
        assert_eq!(split.components, vec!["*.pem".to_owned()]);
    }

    #[test]
    fn split_glob_source_should_normalize_current_dir_components() {
        let split = split("certs/*/./other");

        assert_eq!(split.base, PathBuf::from("certs"));
        assert_eq!(split.components, vec!["*".to_owned(), "other".to_owned()]);
    }

    #[test]
    fn expand_glob_source_should_match_literal_components_between_patterns() {
        let base = temp_base("literal-middle");
        std::fs::create_dir_all(base.join("a/keep")).expect("dirs should be created");
        std::fs::create_dir_all(base.join("b/skip")).expect("dirs should be created");
        std::fs::write(base.join("a/keep/x.pem"), "x").expect("file should be written");
        std::fs::write(base.join("b/skip/y.pem"), "y").expect("file should be written");
        std::fs::create_dir_all(base.join("a/keep/deeper")).expect("dirs should be created");
        std::fs::write(base.join("a/keep/deeper/z.pem"), "z").expect("file should be written");

        assert_eq!(
            expand(&base, "*/keep/*.pem"),
            vec!["a/keep/x.pem"],
            "only paths matching every component at the right depth should match"
        );
    }

    #[test]
    fn expand_glob_source_should_match_case_sensitively() {
        let base = temp_base("case-sensitive");
        std::fs::write(base.join("Upper.pem"), "u").expect("file should be written");

        assert!(
            expand(&base, "upper*").is_empty(),
            "patterns must not match case-insensitively, even on case-insensitive filesystems"
        );
        assert_eq!(expand(&base, "Upper*"), vec!["Upper.pem"]);
    }
}

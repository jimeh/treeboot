use std::path::{Component, Path, PathBuf};

use globset::{GlobBuilder, GlobMatcher};

/// Returns whether a declared source path contains unescaped glob syntax.
pub(crate) fn is_glob_source(source: &Path) -> bool {
    source.components().any(|component| match component {
        Component::Normal(value) => {
            component_has_unescaped_metacharacters(value.to_string_lossy().as_ref())
        }
        _ => false,
    })
}

fn component_has_unescaped_metacharacters(value: &str) -> bool {
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if matches!(character, '*' | '?' | '[') {
            return true;
        }
    }

    false
}

/// A glob source split into its literal base and pattern components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SplitGlobSource {
    /// Declared literal base path, before the first pattern component.
    pub(crate) base: PathBuf,
    /// Pattern components after the base, in declaration order.
    pub(crate) components: Vec<String>,
}

/// Context-agnostic glob source failure, resolved at the caller boundary.
#[derive(Debug)]
pub(crate) enum GlobSourceError {
    /// The pattern contains a `..` component after the base.
    UnsupportedComponent(&'static str),
    /// The pattern is not a valid glob.
    InvalidPattern(globset::Error),
    /// Walking the base directory failed.
    Io {
        /// Directory or entry being inspected.
        path: PathBuf,
        /// Underlying I/O error.
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
pub(crate) fn split_glob_source(source: &Path) -> Result<SplitGlobSource, GlobSourceError> {
    let mut base = PathBuf::new();
    let mut components = Vec::new();

    for component in source.components() {
        match component {
            Component::Normal(value) if components.is_empty() => {
                let value = value.to_string_lossy();
                if component_has_unescaped_metacharacters(value.as_ref()) {
                    components.push(value.into_owned());
                } else {
                    base.push(value.as_ref());
                }
            }
            Component::Normal(value) => components.push(value.to_string_lossy().into_owned()),
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
    /// Whether the match is a directory. Directory symlinks are not directories.
    pub(crate) is_dir: bool,
}

/// Expands a split glob source against the filesystem under `base_path`.
pub(crate) fn expand_glob_source(
    base_path: &Path,
    split: &SplitGlobSource,
) -> Result<Vec<GlobSourceMatch>, GlobSourceError> {
    let segments = split
        .components
        .iter()
        .map(|component| build_segment(component))
        .collect::<Result<Vec<_>, _>>()?;
    let mut matches = Vec::new();

    walk(
        &WalkContext { segments },
        base_path,
        Path::new(""),
        0,
        &mut matches,
    )?;
    matches.sort_by(|left, right| left.relative.cmp(&right.relative));
    matches.dedup_by(|left, right| left.relative == right.relative);

    Ok(matches)
}

struct WalkContext {
    segments: Vec<GlobSegment>,
}

enum GlobSegment {
    Recursive,
    Literal(String),
    Pattern(GlobMatcher),
}

fn build_segment(component: &str) -> Result<GlobSegment, GlobSourceError> {
    if component == "**" {
        return Ok(GlobSegment::Recursive);
    }

    if !component.contains('\\') && !component_has_unescaped_metacharacters(component) {
        return Ok(GlobSegment::Literal(component.to_owned()));
    }

    let pattern = component.replace('{', "[{]").replace('}', "[}]");
    GlobBuilder::new(&pattern)
        .literal_separator(true)
        .backslash_escape(true)
        .build()
        .map(|glob| glob.compile_matcher())
        .map(GlobSegment::Pattern)
        .map_err(GlobSourceError::InvalidPattern)
}

fn walk(
    context: &WalkContext,
    directory: &Path,
    relative: &Path,
    segment_index: usize,
    matches: &mut Vec<GlobSourceMatch>,
) -> Result<(), GlobSourceError> {
    let Some(segment) = context.segments.get(segment_index) else {
        return Ok(());
    };

    if matches!(segment, GlobSegment::Recursive) {
        walk_recursive(context, directory, relative, segment_index, matches)?;
        return Ok(());
    }

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

        if !segment_matches(segment, entry.file_name().to_string_lossy().as_ref()) {
            continue;
        }

        let next_index = segment_index + 1;
        if next_index == context.segments.len() {
            matches.push(GlobSourceMatch {
                relative: entry_relative,
                is_dir,
            });
            continue;
        }

        if is_dir {
            walk(context, &entry_path, &entry_relative, next_index, matches)?;
        }
    }

    Ok(())
}

fn walk_recursive(
    context: &WalkContext,
    directory: &Path,
    relative: &Path,
    segment_index: usize,
    matches: &mut Vec<GlobSourceMatch>,
) -> Result<(), GlobSourceError> {
    let next_index = segment_index + 1;
    if next_index < context.segments.len() {
        walk(context, directory, relative, next_index, matches)?;
    }

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

        if next_index == context.segments.len() {
            matches.push(GlobSourceMatch {
                relative: entry_relative.clone(),
                is_dir,
            });
            continue;
        }

        if is_dir {
            walk_recursive(
                context,
                &entry_path,
                &entry_relative,
                segment_index,
                matches,
            )?;
        }
    }

    Ok(())
}

fn segment_matches(segment: &GlobSegment, file_name: &str) -> bool {
    match segment {
        GlobSegment::Recursive => true,
        GlobSegment::Literal(literal) => file_name == literal,
        GlobSegment::Pattern(matcher) => matcher.is_match(file_name),
    }
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
        expand_glob_source(base, &split(source))
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
    fn is_glob_source_should_detect_unescaped_metacharacters() {
        assert!(is_glob_source(Path::new("certs/*.pem")));
        assert!(is_glob_source(Path::new("file?.txt")));
        assert!(is_glob_source(Path::new("file[0-9].txt")));
        assert!(!is_glob_source(Path::new("certs/a.pem")));
        assert!(!is_glob_source(Path::new(r"certs/\*.pem")));
    }

    #[test]
    fn split_glob_source_should_use_longest_literal_prefix() {
        let split = split("traefik/certs/*.pem");

        assert_eq!(split.base, PathBuf::from("traefik/certs"));
        assert_eq!(split.components, vec!["*.pem".to_owned()]);
    }

    #[test]
    fn split_glob_source_should_reject_parent_components_after_pattern() {
        let error = split_glob_source(Path::new("certs/*/../other")).expect_err("should fail");

        assert!(error.to_string().contains("`..`"));
    }

    #[test]
    fn expand_glob_source_should_match_hidden_files() {
        let base = temp_base("hidden");
        std::fs::write(base.join(".env"), "x").expect("file should be written");

        assert_eq!(expand(&base, "*"), vec![".env"]);
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

        assert_eq!(expand(&base, "**/*.pem"), vec!["real/inner.pem"]);
        assert_eq!(expand(&base, "link*"), vec!["linked"]);
    }

    #[test]
    fn expand_glob_source_should_treat_braces_as_literals() {
        let base = temp_base("braces");
        std::fs::write(base.join("{a}.pem"), "x").expect("file should be written");
        std::fs::write(base.join("b.pem"), "x").expect("file should be written");

        assert_eq!(expand(&base, "{a}*"), vec!["{a}.pem"]);
    }

    #[test]
    fn expand_glob_source_should_match_literal_components_between_patterns() {
        let base = temp_base("literal-middle");
        std::fs::create_dir_all(base.join("a/keep")).expect("dirs should be created");
        std::fs::create_dir_all(base.join("b/skip")).expect("dirs should be created");
        std::fs::write(base.join("a/keep/x.pem"), "x").expect("file should be written");
        std::fs::write(base.join("b/skip/y.pem"), "y").expect("file should be written");
        std::fs::create_dir_all(base.join("a/keep/deeper")).expect("dir should be created");
        std::fs::write(base.join("a/keep/deeper/z.pem"), "z").expect("file should be written");

        assert_eq!(expand(&base, "*/keep/*.pem"), vec!["a/keep/x.pem"]);
    }

    #[test]
    fn expand_glob_source_should_match_single_character_wildcards() {
        let base = temp_base("question-mark");
        std::fs::write(base.join("a1.pem"), "x").expect("file should be written");
        std::fs::write(base.join("a12.pem"), "x").expect("file should be written");

        assert_eq!(expand(&base, "a?.pem"), vec!["a1.pem"]);
    }

    #[test]
    fn expand_glob_source_should_match_escaped_metacharacters_after_patterns() {
        let base = temp_base("escaped");
        std::fs::create_dir_all(base.join("a")).expect("dir should be created");
        std::fs::write(base.join("a/literal*.pem"), "x").expect("file should be written");
        std::fs::write(base.join("a/literal1.pem"), "x").expect("file should be written");

        assert_eq!(expand(&base, r"*/literal\*.pem"), vec!["a/literal*.pem"]);
    }

    #[test]
    fn expand_glob_source_should_match_case_sensitively() {
        let base = temp_base("case-sensitive");
        std::fs::write(base.join("Upper.pem"), "u").expect("file should be written");

        assert!(expand(&base, "upper*").is_empty());
        assert_eq!(expand(&base, "Upper*"), vec!["Upper.pem"]);
    }
}

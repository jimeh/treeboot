use std::path::Path;

use ignore::gitignore::Gitignore;

#[derive(Debug, Clone)]
pub(crate) struct PathIgnoreRules {
    matcher: Gitignore,
    has_negation: bool,
}

impl PathIgnoreRules {
    pub(crate) fn new(root: &Path, patterns: &[String]) -> Result<Self, ignore::Error> {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
        for pattern in patterns {
            builder.add_line(None, pattern)?;
        }
        let matcher = builder.build()?;
        let has_negation = matcher.num_whitelists() > 0;

        Ok(Self {
            matcher,
            has_negation,
        })
    }

    pub(crate) const fn has_negation(&self) -> bool {
        self.has_negation
    }

    pub(crate) fn is_ignored(&self, relative: &Path, is_dir: bool) -> bool {
        self.matcher.matched(relative, is_dir).is_ignore()
    }
}

/// Why an include pattern is rejected during validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IncludePatternIssue {
    Negation,
    Blank,
    Comment,
}

impl IncludePatternIssue {
    pub(crate) fn message(self, pattern: &str) -> String {
        match self {
            Self::Negation => format!(
                "include pattern `{pattern}` uses `!` negation; \
                 use `ignore` to exclude paths"
            ),
            Self::Blank => "include patterns cannot be blank".to_owned(),
            Self::Comment => format!(
                "include pattern `{pattern}` is a gitignore comment; \
                 escape a literal leading `#` as `\\#`"
            ),
        }
    }
}

/// Returns why an include pattern is invalid, or `None` when it is an
/// effective positive pattern. Unlike ignore lists, include lists reject `!`
/// negation, blank entries, and `#` comment lines.
pub(crate) fn invalid_include_pattern(pattern: &str) -> Option<IncludePatternIssue> {
    if pattern.trim().is_empty() {
        return Some(IncludePatternIssue::Blank);
    }
    if pattern.starts_with('!') {
        return Some(IncludePatternIssue::Negation);
    }
    if pattern.starts_with('#') {
        return Some(IncludePatternIssue::Comment);
    }

    None
}

/// Operation-local include rules for copy and sync tree traversal.
///
/// A path is included when it matches any pattern (a plain union); exclusion
/// belongs to ignore rules. Patterns are pre-validated as effective positive
/// patterns via [`invalid_include_pattern`].
#[derive(Debug, Clone)]
pub(crate) struct PathIncludeRules {
    matcher: Gitignore,
    viability: Vec<PatternViability>,
}

#[derive(Debug, Clone)]
enum PatternViability {
    /// The pattern can match at any depth; every directory stays viable.
    Always,
    /// The pattern is anchored; viability walks its path segments.
    Segments(Vec<ViabilitySegment>),
}

#[derive(Debug, Clone)]
enum ViabilitySegment {
    DoubleStar,
    Wildcard,
    Literal(String),
}

impl PathIncludeRules {
    pub(crate) fn new(root: &Path, patterns: &[String]) -> Result<Self, ignore::Error> {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
        for pattern in patterns {
            builder.add_line(None, pattern)?;
        }
        let matcher = builder.build()?;
        let viability = patterns
            .iter()
            .map(|pattern| pattern_viability(pattern))
            .collect();

        Ok(Self { matcher, viability })
    }

    /// Returns whether the source-relative path matches any include pattern.
    pub(crate) fn is_included(&self, relative: &Path, is_dir: bool) -> bool {
        self.matcher.matched(relative, is_dir).is_ignore()
    }

    /// Returns whether a descendant of the source-relative directory could
    /// match an include pattern.
    ///
    /// The answer over-approximates when unsure, so `true` does not guarantee
    /// a match exists, but `false` guarantees no descendant can match and the
    /// directory is safe to prune.
    pub(crate) fn dir_may_contain_matches(&self, relative: &Path) -> bool {
        let Some(components) = utf8_components(relative) else {
            return true;
        };

        self.viability.iter().any(|pattern| match pattern {
            PatternViability::Always => true,
            PatternViability::Segments(segments) => dir_viable(&components, segments),
        })
    }
}

fn pattern_viability(pattern: &str) -> PatternViability {
    let trimmed = pattern.strip_suffix('/').unwrap_or(pattern);
    // Only a leading or middle separator anchors a gitignore pattern; a
    // slash-free pattern matches at any depth and disables pruning.
    if !trimmed.contains('/') {
        return PatternViability::Always;
    }

    let anchored = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let segments = anchored
        .split('/')
        .map(|segment| {
            if segment == "**" {
                ViabilitySegment::DoubleStar
            } else if segment.contains(['*', '?', '[', ']', '{', '}', '\\']) {
                // Non-literal segments are conservatively viable so pruning
                // never under-approximates.
                ViabilitySegment::Wildcard
            } else {
                ViabilitySegment::Literal(segment.to_owned())
            }
        })
        .collect();

    PatternViability::Segments(segments)
}

fn dir_viable(components: &[&str], segments: &[ViabilitySegment]) -> bool {
    let mut segments = segments.iter();
    for component in components {
        match segments.next() {
            // The directory is deeper than the pattern, so the pattern can
            // only match the directory itself or an ancestor, never a strict
            // descendant.
            None => return false,
            Some(ViabilitySegment::DoubleStar) => return true,
            Some(ViabilitySegment::Wildcard) => {}
            Some(ViabilitySegment::Literal(literal)) => {
                if literal != component {
                    return false;
                }
            }
        }
    }

    true
}

fn utf8_components(relative: &Path) -> Option<Vec<&str>> {
    relative
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect()
}

/// Returns whether any planning-reachable entry under `dir_path` is in scope:
/// included, directly or through an included ancestor, and not ignored.
///
/// This keys lazy directory materialization, so it mirrors planning gates
/// exactly. Reachability mirrors tree planning: ignored directories are only
/// entered when the ignore rules use negation, and directories that cannot
/// contain include matches are pruned. Entries that match include but are
/// ignored do not count; they produce no actions, so materializing their
/// ancestors would leave empty scaffold directories. Read errors are
/// conservatively treated as containing matches so planning surfaces them
/// with proper operation context.
pub(crate) fn subtree_contains_included(
    source_root: &Path,
    dir_path: &Path,
    include: &PathIncludeRules,
    ignore: Option<&PathIgnoreRules>,
) -> bool {
    subtree_contains_in_scope(source_root, dir_path, include, ignore, false)
}

fn subtree_contains_in_scope(
    source_root: &Path,
    dir_path: &Path,
    include: &PathIncludeRules,
    ignore: Option<&PathIgnoreRules>,
    ancestor_included: bool,
) -> bool {
    let Ok(entries) = std::fs::read_dir(dir_path) else {
        return true;
    };

    for entry in entries {
        let Ok(entry) = entry else {
            return true;
        };
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(source_root) else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            return true;
        };
        let is_dir = file_type.is_dir();
        let included = ancestor_included || include.is_included(relative, is_dir);
        let ignored = ignore.is_some_and(|rules| rules.is_ignored(relative, is_dir));

        if included && !ignored {
            return true;
        }

        if !is_dir {
            continue;
        }

        let unreachable_ignored_dir = ignored && !ignore.is_some_and(PathIgnoreRules::has_negation);
        if unreachable_ignored_dir || (!included && !include.dir_may_contain_matches(relative)) {
            continue;
        }

        if subtree_contains_in_scope(source_root, &path, include, ignore, included) {
            return true;
        }
    }

    false
}

/// Returns whether any source path below `source_root` matches the include
/// rules, before ignore filtering.
///
/// This feeds the zero-match include warning. Without ignore rules, the
/// in-scope subtree scan reduces to a plain include-match walk, so the same
/// traversal serves both. Read errors conservatively count as a match so the
/// warning heuristic never introduces new failures.
pub(crate) fn include_matches_any_source_path(
    source_root: &Path,
    include: &PathIncludeRules,
) -> bool {
    subtree_contains_included(source_root, source_root, include, None)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn rules(patterns: &[&str]) -> PathIgnoreRules {
        PathIgnoreRules::new(
            Path::new("shared"),
            &patterns.iter().map(ToString::to_string).collect::<Vec<_>>(),
        )
        .expect("patterns should build")
    }

    fn include_rules(patterns: &[&str]) -> PathIncludeRules {
        PathIncludeRules::new(
            Path::new("shared"),
            &patterns.iter().map(ToString::to_string).collect::<Vec<_>>(),
        )
        .expect("patterns should build")
    }

    #[test]
    fn path_ignore_rules_should_match_nested_vendor_descendants() {
        let rules = rules(&["**/vendor/**"]);

        assert!(rules.is_ignored(Path::new("packages/vendor/file"), false));
        assert!(!rules.is_ignored(Path::new("packages/src/file"), false));
    }

    #[test]
    fn path_ignore_rules_should_allow_later_negation() {
        let rules = rules(&["**/vendor/**", "!**/vendor/keep/**"]);

        assert!(rules.has_negation());
        assert!(rules.is_ignored(Path::new("vendor/drop/file"), false));
        assert!(!rules.is_ignored(Path::new("vendor/keep/file"), false));
    }

    #[test]
    fn path_ignore_rules_should_allow_later_ignore_after_negation() {
        let rules = rules(&[
            "**/vendor/**",
            "!**/vendor/keep/**",
            "**/vendor/keep/tmp/**",
        ]);

        assert!(!rules.is_ignored(Path::new("vendor/keep/file"), false));
        assert!(rules.is_ignored(Path::new("vendor/keep/tmp/file"), false));
    }

    #[test]
    fn path_ignore_rules_should_respect_directory_only_patterns() {
        let rules = rules(&["cache/"]);

        assert!(rules.is_ignored(Path::new("cache"), true));
        assert!(!rules.is_ignored(Path::new("cache"), false));
    }

    #[test]
    fn path_ignore_rules_should_accept_comments_and_blank_patterns() {
        let rules = rules(&["# comment", "", "tmp"]);

        assert!(!rules.is_ignored(Path::new("comment"), false));
        assert!(rules.is_ignored(Path::new("tmp"), false));
    }

    #[test]
    fn path_ignore_rules_should_match_escaped_comment_and_negation_prefixes() {
        let rules = rules(&[r"\#literal", r"\!literal"]);

        assert!(rules.is_ignored(Path::new("#literal"), false));
        assert!(rules.is_ignored(Path::new("!literal"), false));
        assert!(!rules.has_negation());
    }

    #[test]
    fn path_ignore_rules_should_reject_invalid_globs() {
        let error = PathIgnoreRules::new(Path::new("shared"), &[String::from("{a,b")])
            .expect_err("invalid glob should fail");

        assert!(error.to_string().contains("{a,b"));
    }

    #[test]
    fn invalid_include_pattern_should_reject_inert_entries() {
        assert_eq!(
            invalid_include_pattern("!docs"),
            Some(IncludePatternIssue::Negation)
        );
        assert_eq!(
            invalid_include_pattern(""),
            Some(IncludePatternIssue::Blank)
        );
        assert_eq!(
            invalid_include_pattern("   "),
            Some(IncludePatternIssue::Blank)
        );
        assert_eq!(
            invalid_include_pattern("# docs only"),
            Some(IncludePatternIssue::Comment)
        );
    }

    #[test]
    fn invalid_include_pattern_should_accept_effective_patterns() {
        assert_eq!(invalid_include_pattern("docs"), None);
        assert_eq!(invalid_include_pattern("**/*.rs"), None);
        assert_eq!(invalid_include_pattern(r"\!literal"), None);
        assert_eq!(invalid_include_pattern(r"\#literal"), None);
    }

    #[test]
    fn path_include_rules_should_match_as_plain_union() {
        let rules = include_rules(&["docs/**", "*.toml"]);

        assert!(rules.is_included(Path::new("docs/guide.md"), false));
        assert!(rules.is_included(Path::new("nested/app.toml"), false));
        assert!(!rules.is_included(Path::new("src/main.rs"), false));
    }

    #[test]
    fn path_include_rules_should_respect_directory_only_patterns() {
        let rules = include_rules(&["docs/"]);

        assert!(rules.is_included(Path::new("docs"), true));
        assert!(!rules.is_included(Path::new("docs"), false));
    }

    #[test]
    fn path_include_rules_should_match_escaped_prefixes() {
        let rules = include_rules(&[r"\!literal", r"\#literal"]);

        assert!(rules.is_included(Path::new("!literal"), false));
        assert!(rules.is_included(Path::new("#literal"), false));
    }

    #[test]
    fn path_include_rules_should_reject_invalid_globs() {
        let error = PathIncludeRules::new(Path::new("shared"), &[String::from("{a,b")])
            .expect_err("invalid glob should fail");

        assert!(error.to_string().contains("{a,b"));
    }

    #[test]
    fn viability_should_walk_anchored_literal_prefixes() {
        let rules = include_rules(&["configs/app/*.toml"]);

        assert!(rules.dir_may_contain_matches(Path::new("configs")));
        assert!(rules.dir_may_contain_matches(Path::new("configs/app")));
        assert!(!rules.dir_may_contain_matches(Path::new("vendor")));
        assert!(!rules.dir_may_contain_matches(Path::new("configs/other")));
        // The `*.toml` segment is a conservative wildcard, so directories at
        // that depth stay viable even though only files can match.
        assert!(rules.dir_may_contain_matches(Path::new("configs/app/deep")));
        assert!(!rules.dir_may_contain_matches(Path::new("configs/app/deep/er")));
    }

    #[test]
    fn viability_should_over_approximate_after_double_star() {
        let rules = include_rules(&["a/**/b/*.toml"]);

        assert!(rules.dir_may_contain_matches(Path::new("a")));
        assert!(rules.dir_may_contain_matches(Path::new("a/x")));
        assert!(rules.dir_may_contain_matches(Path::new("a/x/b")));
        assert!(!rules.dir_may_contain_matches(Path::new("c")));
    }

    #[test]
    fn viability_should_treat_slash_free_patterns_as_always_viable() {
        let rules = include_rules(&["*.rs"]);

        assert!(rules.dir_may_contain_matches(Path::new("anything")));
        assert!(rules.dir_may_contain_matches(Path::new("deep/nested/dir")));
    }

    #[test]
    fn viability_should_treat_trailing_slash_only_patterns_as_unanchored() {
        let rules = include_rules(&["docs/"]);

        assert!(rules.dir_may_contain_matches(Path::new("nested")));
    }

    #[test]
    fn viability_should_treat_wildcard_segments_conservatively() {
        let rules = include_rules(&["a/*/c"]);

        assert!(rules.dir_may_contain_matches(Path::new("a/anything")));
        assert!(!rules.dir_may_contain_matches(Path::new("a/x/y")));
        assert!(!rules.dir_may_contain_matches(Path::new("b")));
    }

    #[test]
    fn viability_should_treat_brace_segments_conservatively() {
        let rules = include_rules(&["configs/{a,b}/app.toml"]);

        assert!(rules.dir_may_contain_matches(Path::new("configs")));
        assert!(rules.dir_may_contain_matches(Path::new("configs/a")));
        assert!(rules.dir_may_contain_matches(Path::new("configs/other")));
        assert!(!rules.dir_may_contain_matches(Path::new("vendor")));
    }

    #[test]
    fn viability_should_stop_below_pattern_depth() {
        let rules = include_rules(&["a/b"]);

        assert!(rules.dir_may_contain_matches(Path::new("a")));
        assert!(rules.dir_may_contain_matches(Path::new("a/b")));
        assert!(!rules.dir_may_contain_matches(Path::new("a/b/c")));
    }

    #[test]
    fn viability_should_union_across_patterns() {
        let rules = include_rules(&["docs/**", "configs/app.toml"]);

        assert!(rules.dir_may_contain_matches(Path::new("docs")));
        assert!(rules.dir_may_contain_matches(Path::new("configs")));
        assert!(!rules.dir_may_contain_matches(Path::new("vendor")));
    }
}

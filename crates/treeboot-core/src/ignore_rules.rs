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
}

use std::ffi::OsStr;
use std::path::{Component, Path};

use sha2::{Digest, Sha256};

use crate::{WorktreeIdConfig, WorktreeSlugConfig};

const HASH_DOMAIN: &[u8] = b"treeboot-worktree-id-v1";
const CROCKFORD: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorktreeIdentity {
    pub(crate) id: String,
    pub(crate) slug: String,
}

pub(crate) fn identity(
    worktree_path: &Path,
    main_worktree_path: Option<&Path>,
    id_config: &WorktreeIdConfig,
    slug_config: &WorktreeSlugConfig,
) -> WorktreeIdentity {
    let encoded = encode_digest(&path_digest(worktree_path));
    let id = encoded[..id_config.length()].to_owned();
    let selected = readable_source(worktree_path);
    let selected =
        if main_worktree_path.is_some() && selected.len() == 1 && is_mechanical(selected[0]) {
            Vec::new()
        } else {
            selected
        };
    let mut readable = sanitize_components(&selected, slug_config.separator());
    if readable.is_empty() {
        readable = main_worktree_path
            .and_then(Path::file_name)
            .map_or_else(String::new, |name| {
                sanitize_components(&[name], slug_config.separator())
            });
    }
    if readable.is_empty() {
        readable = worktree_path.file_name().map_or_else(String::new, |name| {
            sanitize_components(&[name], slug_config.separator())
        });
    }
    if readable.is_empty() {
        readable = "worktree".to_owned();
    }

    let budget = slug_config.max_length().saturating_sub(id.len() + 1);
    if readable.len() > budget {
        readable.truncate(budget);
        while readable.ends_with(slug_config.separator()) {
            readable.pop();
        }
    }
    if readable.is_empty() {
        readable = fallback_within_budget(
            main_worktree_path.unwrap_or(worktree_path),
            slug_config.separator(),
            budget,
        );
    }

    let slug = format!("{readable}{}{id}", slug_config.separator());
    WorktreeIdentity { id, slug }
}

fn fallback_within_budget(main_worktree_path: &Path, separator: char, budget: usize) -> String {
    let mut fallback = main_worktree_path
        .file_name()
        .map_or_else(String::new, |name| sanitize_components(&[name], separator));
    if fallback.is_empty() {
        fallback = "worktree".to_owned();
    }
    fallback.truncate(budget);
    while fallback.ends_with(separator) {
        fallback.pop();
    }
    if fallback.is_empty() {
        "worktree"[..budget.min("worktree".len())].to_owned()
    } else {
        fallback
    }
}

fn readable_source(path: &Path) -> Vec<&OsStr> {
    let components = normal_components(path);
    let len = components.len();
    if len >= 4
        && components[len - 4] == OsStr::new(".codex")
        && components[len - 3] == OsStr::new("worktrees")
    {
        return vec![components[len - 1]];
    }
    if len >= 4
        && components[len - 3] == OsStr::new(".claude")
        && components[len - 2] == OsStr::new("worktrees")
    {
        return vec![components[len - 1]];
    }
    if len >= 4
        && components[len - 4] == OsStr::new(".t3")
        && components[len - 3] == OsStr::new("worktrees")
        && is_t3code_name(components[len - 1])
    {
        return vec![components[len - 2]];
    }
    if len >= 4
        && components[len - 4] == OsStr::new("conductor")
        && components[len - 3] == OsStr::new("workspaces")
    {
        return vec![components[len - 2], components[len - 1]];
    }
    if let Some(index) = components
        .windows(2)
        .enumerate()
        .rev()
        .find_map(|(index, window)| {
            (window[0] == OsStr::new(".superset")
                && window[1] == OsStr::new("worktrees")
                && index + 3 < len)
                .then_some(index)
        })
    {
        return components[index + 3..].to_vec();
    }

    components.last().copied().into_iter().collect()
}

fn normal_components(path: &Path) -> Vec<&OsStr> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value),
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => None,
        })
        .collect()
}

fn is_mechanical(value: &OsStr) -> bool {
    let Some(value) = value.to_str() else {
        return false;
    };
    is_uuid(value)
        || (value.len() >= 8 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        || is_t3code_token(value)
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

fn is_t3code_name(value: &OsStr) -> bool {
    value.to_str().is_some_and(is_t3code_token)
}

fn is_t3code_token(value: &str) -> bool {
    value.strip_prefix("t3code-").is_some_and(|token| {
        !token.is_empty()
            && token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    })
}

fn sanitize_components(components: &[&OsStr], separator: char) -> String {
    let mut output = String::new();
    let mut unsupported = false;
    for (component_index, component) in components.iter().enumerate() {
        if component_index > 0 {
            unsupported = true;
        }
        for character in component.to_string_lossy().chars() {
            if character.is_ascii_alphanumeric() {
                if unsupported && !output.is_empty() {
                    output.push(separator);
                }
                output.push(character.to_ascii_lowercase());
                unsupported = false;
            } else {
                unsupported = true;
            }
        }
    }
    output
}

fn path_digest(path: &Path) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(HASH_DOMAIN);
    hasher.update([0]);
    update_platform_path(&mut hasher, path.as_os_str());
    hasher.finalize().into()
}

#[cfg(unix)]
fn update_platform_path(hasher: &mut Sha256, path: &OsStr) {
    use std::os::unix::ffi::OsStrExt;

    hasher.update(b"unix");
    hasher.update([0]);
    hasher.update(path.as_bytes());
}

#[cfg(windows)]
fn update_platform_path(hasher: &mut Sha256, path: &OsStr) {
    use std::os::windows::ffi::OsStrExt;

    hasher.update(b"windows");
    hasher.update([0]);
    for unit in path.encode_wide() {
        hasher.update(unit.to_le_bytes());
    }
}

#[cfg(not(any(unix, windows)))]
fn update_platform_path(hasher: &mut Sha256, path: &OsStr) {
    hasher.update(b"unknown");
    hasher.update([0]);
    hasher.update(path.to_string_lossy().as_bytes());
}

fn encode_digest(digest: &[u8; 32]) -> String {
    let mut encoded = String::with_capacity(52);
    let mut accumulator = 0_u16;
    let mut bits = 0_u8;
    for byte in digest {
        accumulator = (accumulator << 8) | u16::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let index = usize::from((accumulator >> bits) & 0x1f);
            encoded.push(char::from(CROCKFORD[index]));
            accumulator &= (1_u16 << bits) - 1;
        }
    }
    if bits > 0 {
        let index = usize::from((accumulator << (5 - bits)) & 0x1f);
        encoded.push(char::from(CROCKFORD[index]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_slug(path: &str, root: &str) -> String {
        identity(
            Path::new(path),
            Some(Path::new(root)),
            &WorktreeIdConfig::default(),
            &WorktreeSlugConfig::default(),
        )
        .slug
    }

    #[test]
    fn identifier_should_recognize_manager_layouts() {
        let cases = [
            ("/home/a/.codex/worktrees/93d1/treeboot", "treeboot-"),
            (
                "/home/a/treeboot/.claude/worktrees/feature-auth",
                "feature-auth-",
            ),
            ("/home/a/.t3/worktrees/treeboot/t3code-ab12", "treeboot-"),
            (
                "/home/a/conductor/workspaces/payments/london",
                "payments-london-",
            ),
            (
                "/home/a/.superset/worktrees/treeboot/owner/feature-x",
                "owner-feature-x-",
            ),
        ];

        for (path, prefix) in cases {
            let value = default_slug(path, "/home/a/treeboot");
            assert!(value.starts_with(prefix), "{path}: {value}");
        }
    }

    #[test]
    fn identifier_should_use_generic_name_for_recognizer_near_miss() {
        let conductor = default_slug(
            "/home/a/conductor2/workspaces/payments/london",
            "/home/a/project",
        );
        let t3 = default_slug(
            "/home/a/.t3/worktrees/payments/feature-auth",
            "/home/a/project",
        );
        let superset = default_slug(
            "/home/a/.superset2/worktrees/payments/owner/feature-auth",
            "/home/a/project",
        );

        assert!(conductor.starts_with("london-"));
        assert!(!conductor.starts_with("payments-london-"));
        assert!(t3.starts_with("feature-auth-"));
        assert!(!t3.starts_with("payments-"));
        assert!(superset.starts_with("feature-auth-"));
        assert!(!superset.starts_with("owner-feature-auth-"));
    }

    #[test]
    fn identifier_should_use_earlier_complete_superset_marker() {
        let value = default_slug(
            "/a/.superset/worktrees/proj/ws/.superset/worktrees/x",
            "/a/proj",
        );

        assert!(value.starts_with("ws-superset-worktrees-x-"), "{value}");
    }

    #[test]
    fn identifier_should_recognize_codex_uuid_parent_and_superset_simple_workspace() {
        let codex = default_slug(
            "/custom/.codex/worktrees/550e8400-e29b-41d4-a716-446655440000/treeboot",
            "/custom/treeboot",
        );
        let superset = default_slug(
            "/custom/.superset/worktrees/treeboot/feature-auth",
            "/custom/treeboot",
        );

        assert!(codex.starts_with("treeboot-"));
        assert!(superset.starts_with("feature-auth-"));
    }

    #[test]
    fn identifier_should_fall_back_for_unknown_mechanical_basenames() {
        for name in ["deadbeef", "t3code-ab12"] {
            let value = default_slug(&format!("/tmp/{name}"), "/tmp/payments");
            assert!(value.starts_with("payments-"), "{name}: {value}");
        }
    }

    #[test]
    fn identifier_should_fall_back_for_mechanical_single_component() {
        let value = default_slug(
            "/home/a/project/.claude/worktrees/550e8400-e29b-41d4-a716-446655440000",
            "/home/a/project",
        );

        assert!(value.starts_with("project-"));
    }

    #[test]
    fn identifier_should_collapse_unsupported_runs() {
        let value = default_slug("/home/a/Feature...  Login", "/home/a/project");

        assert!(value.starts_with("feature-login-"));
    }

    #[test]
    fn identifier_should_truncate_without_trailing_separator() {
        let value = identity(
            Path::new("/home/a/abcdefghi--jkl"),
            Some(Path::new("/home/a/project")),
            &WorktreeIdConfig::default(),
            &WorktreeSlugConfig::new(16, '-').expect("config should be valid"),
        )
        .slug;

        assert_eq!(value.len(), 16);
        assert!(!value[..9].ends_with('-'));
    }

    #[test]
    fn identifier_should_use_final_fallback_when_names_sanitize_empty() {
        let value = default_slug("/tmp/💥", "/tmp/✨");

        assert!(value.starts_with("worktree-"));
    }

    #[test]
    fn identifier_should_use_main_worktree_when_selected_name_sanitizes_empty() {
        let value = default_slug("/tmp/💥", "/tmp/payments");

        assert!(value.starts_with("payments-"));
    }

    #[test]
    fn identifier_should_support_underscore_separator_and_exact_minimum_length() {
        let value = identity(
            Path::new("/tmp/long-readable-name"),
            Some(Path::new("/tmp/project")),
            &WorktreeIdConfig::default(),
            &WorktreeSlugConfig::new(8, '_').expect("config should be valid"),
        )
        .slug;

        assert_eq!(value.len(), 8);
        assert_eq!(value.as_bytes()[1], b'_');
    }

    #[test]
    fn identifier_should_distinguish_same_names_under_different_parents() {
        let first = default_slug("/tmp/one/feature", "/tmp/one/project");
        let second = default_slug("/tmp/two/feature", "/tmp/two/project");

        assert_ne!(first, second);
        assert!(first.starts_with("feature-"));
        assert!(second.starts_with("feature-"));
    }

    #[test]
    fn identifier_should_hash_native_path_spelling_without_case_folding() {
        let upper = default_slug("/tmp/Feature", "/tmp/project");
        let lower = default_slug("/tmp/feature", "/tmp/project");

        assert_ne!(upper, lower);
        assert!(upper.starts_with("feature-"));
        assert!(lower.starts_with("feature-"));
    }

    #[test]
    fn identifier_should_keep_digest_prefix_across_presentation_settings() {
        let path = Path::new("/tmp/feature-login");
        let root = Path::new("/tmp/project");
        let short = identity(
            path,
            Some(root),
            &WorktreeIdConfig::new(6).expect("config should be valid"),
            &WorktreeSlugConfig::new(48, '-').expect("config should be valid"),
        );
        let long = identity(
            path,
            Some(root),
            &WorktreeIdConfig::new(12).expect("config should be valid"),
            &WorktreeSlugConfig::new(80, '_').expect("config should be valid"),
        );

        assert_eq!(short.id, &long.id[..6]);
        assert!(short.slug.ends_with(&short.id));
        assert!(long.slug.ends_with(&long.id));
    }

    #[test]
    fn id_should_ignore_readable_fallback_separator_and_slug_maximum() {
        let path = Path::new("/tmp/deadbeef");
        let id_config = WorktreeIdConfig::new(10).expect("ID config should be valid");
        let first = identity(
            path,
            Some(Path::new("/tmp/project-one")),
            &id_config,
            &WorktreeSlugConfig::new(24, '-').expect("slug config should be valid"),
        );
        let second = identity(
            path,
            None,
            &id_config,
            &WorktreeSlugConfig::new(64, '_').expect("slug config should be valid"),
        );

        assert_eq!(first.id, second.id);
        assert_ne!(first.slug, second.slug);
        assert!(first.slug.ends_with(&first.id));
        assert!(second.slug.ends_with(&second.id));
    }

    #[test]
    fn slug_should_use_deterministic_non_git_fallbacks() {
        let mechanical = identity(
            Path::new("/tmp/deadbeef"),
            None,
            &WorktreeIdConfig::default(),
            &WorktreeSlugConfig::default(),
        );
        let empty = identity(
            Path::new("/tmp/💥"),
            None,
            &WorktreeIdConfig::default(),
            &WorktreeSlugConfig::default(),
        );

        assert!(mechanical.slug.starts_with("deadbeef-"));
        assert!(mechanical.slug.ends_with(&mechanical.id));
        assert!(empty.slug.starts_with("worktree-"));
        assert!(empty.slug.ends_with(&empty.id));
    }

    #[test]
    fn identifier_default_should_match_dns_label_contract() {
        let value = default_slug("/tmp/--Feature Login--", "/tmp/project");

        assert!(value.len() <= 48);
        assert!(
            value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        );
        assert!(
            value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
        );
        assert!(
            value
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
        );
    }

    #[test]
    fn encode_digest_should_emit_52_crockford_characters() {
        let encoded = encode_digest(&[0xff; 32]);

        assert_eq!(
            encoded,
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzg"
        );
    }

    #[cfg(unix)]
    #[test]
    fn identifier_should_match_fixed_unix_path_vector() {
        let value = identity(
            Path::new("/home/alice/worktrees/payments/feature-login"),
            Some(Path::new("/home/alice/worktrees/payments")),
            &WorktreeIdConfig::default(),
            &WorktreeSlugConfig::default(),
        );

        assert_eq!(value.id, "a20v6r");
        assert_eq!(value.slug, "feature-login-a20v6r");
    }

    #[cfg(unix)]
    #[test]
    fn identifier_should_expose_full_fixed_digest_through_checked_config() {
        let value = identity(
            Path::new("/home/alice/worktrees/payments/feature-login"),
            Some(Path::new("/home/alice/worktrees/payments")),
            &WorktreeIdConfig::new(52).expect("config should be valid"),
            &WorktreeSlugConfig::new(80, '-').expect("config should be valid"),
        );

        assert_eq!(
            value.id,
            "a20v6rkf7fqtm08dt0tfx2bm8cy0w7xdrytn08wctah6htqhrr30"
        );
    }

    #[cfg(unix)]
    #[test]
    fn identifier_should_hash_non_utf8_native_path_bytes() {
        use std::os::unix::ffi::OsStringExt;

        let path =
            std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/worktree-\xff".to_vec()));
        let first = identity(
            &path,
            Some(Path::new("/tmp/project")),
            &WorktreeIdConfig::default(),
            &WorktreeSlugConfig::default(),
        );
        let second = identity(
            Path::new("/tmp/worktree-"),
            Some(Path::new("/tmp/project")),
            &WorktreeIdConfig::default(),
            &WorktreeSlugConfig::default(),
        );

        assert_ne!(first.id, second.id);
    }

    #[cfg(windows)]
    #[test]
    fn identifier_should_match_fixed_windows_native_path_vector() {
        let encoded = encode_digest(&path_digest(Path::new(r"C:\repo\feature")));

        assert_eq!(
            encoded,
            "yd6jgc6s50tehhbk3ztd0qmp7ap4py29v38062gb41nag2fm5p3g"
        );
    }
}

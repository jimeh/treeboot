use std::ffi::OsStr;
use std::path::{Component, Path};

use sha2::{Digest, Sha256};

use crate::WorktreeIdConfig;

const HASH_DOMAIN: &[u8] = b"treeboot-worktree-id-v1";
const CROCKFORD: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

pub(crate) fn identifier(
    worktree_path: &Path,
    main_worktree_path: &Path,
    config: &WorktreeIdConfig,
) -> String {
    let selected = readable_source(worktree_path);
    let selected = if selected.len() == 1 && is_mechanical(selected[0]) {
        Vec::new()
    } else {
        selected
    };
    let mut readable = sanitize_components(&selected, config.separator());
    if readable.is_empty() {
        readable = main_worktree_path
            .file_name()
            .map_or_else(String::new, |name| {
                sanitize_components(&[name], config.separator())
            });
    }
    if readable.is_empty() {
        readable = "worktree".to_owned();
    }

    let budget = config.max_length() - config.hash_length() - 1;
    if readable.len() > budget {
        readable.truncate(budget);
        while readable.ends_with(config.separator()) {
            readable.pop();
        }
    }
    if readable.is_empty() {
        readable = fallback_within_budget(main_worktree_path, config.separator(), budget);
    }

    let encoded = encode_digest(&path_digest(worktree_path));
    format!(
        "{readable}{}{hash}",
        config.separator(),
        hash = &encoded[..config.hash_length()]
    )
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
        .rposition(|window| {
            window[0] == OsStr::new(".superset") && window[1] == OsStr::new("worktrees")
        })
        .filter(|index| index + 3 < len)
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

    fn default_id(path: &str, root: &str) -> String {
        identifier(
            Path::new(path),
            Path::new(root),
            &WorktreeIdConfig::default(),
        )
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
            let value = default_id(path, "/home/a/treeboot");
            assert!(value.starts_with(prefix), "{path}: {value}");
        }
    }

    #[test]
    fn identifier_should_use_generic_name_for_recognizer_near_miss() {
        let value = default_id(
            "/home/a/conductor2/workspaces/payments/london",
            "/home/a/project",
        );

        assert!(value.starts_with("london-"));
        assert!(!value.starts_with("payments-london-"));
    }

    #[test]
    fn identifier_should_recognize_codex_uuid_parent_and_superset_simple_workspace() {
        let codex = default_id(
            "/custom/.codex/worktrees/550e8400-e29b-41d4-a716-446655440000/treeboot",
            "/custom/treeboot",
        );
        let superset = default_id(
            "/custom/.superset/worktrees/treeboot/feature-auth",
            "/custom/treeboot",
        );

        assert!(codex.starts_with("treeboot-"));
        assert!(superset.starts_with("feature-auth-"));
    }

    #[test]
    fn identifier_should_fall_back_for_unknown_mechanical_basenames() {
        for name in ["deadbeef", "t3code-ab12"] {
            let value = default_id(&format!("/tmp/{name}"), "/tmp/payments");
            assert!(value.starts_with("payments-"), "{name}: {value}");
        }
    }

    #[test]
    fn identifier_should_fall_back_for_mechanical_single_component() {
        let value = default_id(
            "/home/a/project/.claude/worktrees/550e8400-e29b-41d4-a716-446655440000",
            "/home/a/project",
        );

        assert!(value.starts_with("project-"));
    }

    #[test]
    fn identifier_should_collapse_unsupported_runs() {
        let value = default_id("/home/a/Feature...  Login", "/home/a/project");

        assert!(value.starts_with("feature-login-"));
    }

    #[test]
    fn identifier_should_truncate_without_trailing_separator() {
        let config = WorktreeIdConfig::new(16, 6, '-').expect("config should be valid");
        let value = identifier(
            Path::new("/home/a/abcdefghi--jkl"),
            Path::new("/home/a/project"),
            &config,
        );

        assert_eq!(value.len(), 16);
        assert!(!value[..9].ends_with('-'));
    }

    #[test]
    fn identifier_should_use_final_fallback_when_names_sanitize_empty() {
        let value = default_id("/tmp/💥", "/tmp/✨");

        assert!(value.starts_with("worktree-"));
    }

    #[test]
    fn identifier_should_use_main_worktree_when_selected_name_sanitizes_empty() {
        let value = default_id("/tmp/💥", "/tmp/payments");

        assert!(value.starts_with("payments-"));
    }

    #[test]
    fn identifier_should_support_underscore_separator_and_exact_minimum_length() {
        let config = WorktreeIdConfig::new(8, 6, '_').expect("config should be valid");
        let value = identifier(
            Path::new("/tmp/long-readable-name"),
            Path::new("/tmp/project"),
            &config,
        );

        assert_eq!(value.len(), 8);
        assert_eq!(value.as_bytes()[1], b'_');
    }

    #[test]
    fn identifier_should_distinguish_same_names_under_different_parents() {
        let first = default_id("/tmp/one/feature", "/tmp/one/project");
        let second = default_id("/tmp/two/feature", "/tmp/two/project");

        assert_ne!(first, second);
        assert!(first.starts_with("feature-"));
        assert!(second.starts_with("feature-"));
    }

    #[test]
    fn identifier_should_hash_native_path_spelling_without_case_folding() {
        let upper = default_id("/tmp/Feature", "/tmp/project");
        let lower = default_id("/tmp/feature", "/tmp/project");

        assert_ne!(upper, lower);
        assert!(upper.starts_with("feature-"));
        assert!(lower.starts_with("feature-"));
    }

    #[test]
    fn identifier_should_keep_digest_prefix_across_presentation_settings() {
        let path = Path::new("/tmp/feature-login");
        let root = Path::new("/tmp/project");
        let short = identifier(
            path,
            root,
            &WorktreeIdConfig::new(48, 6, '-').expect("config should be valid"),
        );
        let long = identifier(
            path,
            root,
            &WorktreeIdConfig::new(80, 12, '_').expect("config should be valid"),
        );

        let short_hash = short.rsplit_once('-').map(|(_, hash)| hash);
        let long_hash = long.rsplit_once('_').map(|(_, hash)| hash);
        assert_eq!(short_hash, long_hash.map(|hash| &hash[..6]));
    }

    #[test]
    fn identifier_default_should_match_dns_label_contract() {
        let value = default_id("/tmp/--Feature Login--", "/tmp/project");

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
        let value = default_id(
            "/home/alice/worktrees/payments/feature-login",
            "/home/alice/worktrees/payments",
        );

        assert_eq!(value, "feature-login-a20v6r");
    }

    #[cfg(unix)]
    #[test]
    fn identifier_should_expose_full_fixed_digest_through_checked_config() {
        let config = WorktreeIdConfig::new(80, 52, '-').expect("config should be valid");
        let value = identifier(
            Path::new("/home/alice/worktrees/payments/feature-login"),
            Path::new("/home/alice/worktrees/payments"),
            &config,
        );

        assert_eq!(
            value,
            "feature-login-a20v6rkf7fqtm08dt0tfx2bm8cy0w7xdrytn08wctah6htqhrr30"
        );
    }

    #[cfg(unix)]
    #[test]
    fn identifier_should_hash_non_utf8_native_path_bytes() {
        use std::os::unix::ffi::OsStringExt;

        let path =
            std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/worktree-\xff".to_vec()));
        let first = identifier(
            &path,
            Path::new("/tmp/project"),
            &WorktreeIdConfig::default(),
        );
        let second = default_id("/tmp/worktree-", "/tmp/project");

        assert_ne!(
            first.rsplit_once('-').map(|(_, hash)| hash),
            second.rsplit_once('-').map(|(_, hash)| hash)
        );
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

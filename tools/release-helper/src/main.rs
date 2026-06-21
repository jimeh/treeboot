use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Parser, Subcommand};
use markdown::mdast::{Heading, Node};
use markdown::{ParseOptions, to_mdast};
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

/// Top-level release helper command-line arguments.
#[derive(Debug, Parser)]
#[command(name = "treeboot-release-helper")]
#[command(about = "Release workflow helper for treeboot")]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

/// Release helper subcommands used by workflow wrapper scripts.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Derive a release version from an override, GitHub tag ref, or git describe.
    Version {
        /// Version override, with or without a leading v.
        #[arg(long, env = "TREEBOOT_RELEASE_VERSION")]
        version: Option<String>,
        /// GitHub ref type.
        #[arg(long, env = "GITHUB_REF_TYPE")]
        github_ref_type: Option<String>,
        /// GitHub ref name.
        #[arg(long, env = "GITHUB_REF_NAME")]
        github_ref_name: Option<String>,
        /// Output GitHub Actions step outputs to this file.
        #[arg(long, env = "GITHUB_OUTPUT")]
        github_output: Option<PathBuf>,
    },
    /// Package a target release binary into dist assets.
    Package {
        /// Rust target triple.
        target: String,
        /// Release version, without the v prefix.
        version: String,
        /// Output directory for release assets.
        dist_dir: PathBuf,
    },
    /// Extract release notes for a version from CHANGELOG.md.
    Notes {
        /// Release version, with or without a leading v.
        version: String,
        /// Output file for the extracted release notes.
        output: PathBuf,
        /// Changelog path.
        #[arg(long, default_value = "CHANGELOG.md")]
        changelog: PathBuf,
    },
}

/// Run the helper and report errors in a workflow-friendly format.
fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

/// Dispatch the selected release helper subcommand.
fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Args::parse().command {
        Commands::Version {
            version,
            github_ref_type,
            github_ref_name,
            github_output,
        } => {
            let version = resolve_version(
                version.as_deref(),
                github_ref_type.as_deref(),
                github_ref_name.as_deref(),
            )?;
            if let Some(output) = github_output {
                write_version_outputs(&version, &output)?;
            } else {
                println!("{}", version.version);
            }
        }
        Commands::Package {
            target,
            version,
            dist_dir,
        } => package_release_asset(&target, &version, &dist_dir)?,
        Commands::Notes {
            version,
            output,
            changelog,
        } => {
            let notes = release_notes(&changelog, &version)?;
            fs::write(output, notes)?;
        }
    }

    Ok(())
}

/// Resolved release version metadata shared across workflow jobs.
#[derive(Debug, Eq, PartialEq)]
struct VersionInfo {
    tag: String,
    version: String,
    safe_version: String,
    is_tag: bool,
}

/// Resolve the release version from explicit input, GitHub tag context, or Git.
fn resolve_version(
    override_version: Option<&str>,
    github_ref_type: Option<&str>,
    github_ref_name: Option<&str>,
) -> Result<VersionInfo, Box<dyn std::error::Error>> {
    if let Some(version) = override_version.filter(|value| !value.is_empty()) {
        let version = version.trim_start_matches('v').to_owned();
        return Ok(version_info(format!("v{version}"), version, false));
    }

    if github_ref_type == Some("tag") {
        let ref_name = github_ref_name.ok_or("missing github ref name for tag event")?;
        let version = ref_name
            .strip_prefix('v')
            .ok_or_else(|| format!("invalid release tag '{ref_name}', expected vX.Y.Z"))?;
        if !is_strict_release_version(version) {
            return Err(format!("invalid release tag '{ref_name}', expected vX.Y.Z").into());
        }

        let version = version.to_owned();
        return Ok(version_info(format!("v{version}"), version, true));
    }

    let describe = git_output([
        "describe", "--tags", "--dirty", "--always", "--match", "v[0-9]*",
    ])
    .or_else(|_| git_output(["rev-parse", "--short", "HEAD"]))?;
    let version = describe.trim_start_matches('v').to_owned();
    let tag = format!("v{version}");

    Ok(version_info(tag, version, false))
}

/// Build the normalized version metadata used by release workflow outputs.
fn version_info(tag: String, version: String, is_tag: bool) -> VersionInfo {
    let safe_version = version
        .chars()
        .map(|character| match character {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '_' | '-' => character,
            _ => '-',
        })
        .collect();

    VersionInfo {
        tag,
        version,
        safe_version,
        is_tag,
    }
}

/// Return whether a tag version has the strict numeric X.Y.Z release shape.
fn is_strict_release_version(version: &str) -> bool {
    let mut parts = version.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(major), Some(minor), Some(patch), None)
            if !major.is_empty()
                && !minor.is_empty()
                && !patch.is_empty()
                && major.chars().all(|character| character.is_ascii_digit())
                && minor.chars().all(|character| character.is_ascii_digit())
                && patch.chars().all(|character| character.is_ascii_digit())
    )
}

/// Run a Git command and return trimmed UTF-8 stdout.
fn git_output<I, S>(args: I) -> Result<String, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err(format!("git command failed with status {}", output.status).into());
    }

    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

/// Append release version metadata to the GitHub Actions output file.
fn write_version_outputs(
    version: &VersionInfo,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(output)?;
    writeln!(file, "tag={}", version.tag)?;
    writeln!(file, "version={}", version.version)?;
    writeln!(file, "safe_version={}", version.safe_version)?;
    writeln!(file, "is_tag={}", version.is_tag)?;

    Ok(())
}

/// Package a built treeboot binary into raw and archived release assets.
fn package_release_asset(
    target: &str,
    version: &str,
    dist_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    package_release_asset_in_project(Path::new("."), target, version, dist_dir)
}

/// Package release assets from a specific project root.
fn package_release_asset_in_project(
    project_dir: &Path,
    target: &str,
    version: &str,
    dist_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let windows = target.contains("windows");
    let exe_suffix = if windows { ".exe" } else { "" };
    let binary = project_dir.join(format!("target/{target}/release/treeboot{exe_suffix}"));
    if !binary.is_file() {
        return Err(format!("missing release binary: {}", binary.display()).into());
    }

    let dist_dir = resolve_project_path(project_dir, dist_dir);
    fs::create_dir_all(&dist_dir)?;
    let raw_asset = dist_dir.join(format!("treeboot-{target}{exe_suffix}"));
    fs::copy(&binary, &raw_asset)?;
    make_executable(&raw_asset)?;

    let temp_dir = env::temp_dir().join(format!(
        "treeboot-release-{}-{}-{}",
        std::process::id(),
        target,
        version
    ));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }
    fs::create_dir_all(&temp_dir)?;

    let payload_dir = temp_dir.join(format!("treeboot-{version}-{target}"));
    fs::create_dir_all(&payload_dir)?;
    let payload_binary = payload_dir.join(format!("treeboot{exe_suffix}"));
    fs::copy(&binary, &payload_binary)?;
    make_executable(&payload_binary)?;
    fs::copy(project_dir.join("README.md"), payload_dir.join("README.md"))?;
    fs::copy(project_dir.join("LICENSE"), payload_dir.join("LICENSE"))?;

    let result = if windows {
        create_zip_archive(
            &dist_dir.join(format!("treeboot-{target}.zip")),
            &payload_dir,
            &format!("treeboot-{version}-{target}"),
        )
    } else {
        create_tar_archive(
            &dist_dir.join(format!("treeboot-{target}.tar.gz")),
            &payload_dir,
            &format!("treeboot-{version}-{target}"),
        )
    };

    fs::remove_dir_all(&temp_dir)?;
    result?;

    Ok(())
}

/// Resolve a path relative to the project root unless it is already absolute.
fn resolve_project_path(project_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        project_dir.join(path)
    }
}

#[cfg(unix)]
/// Mark a packaged Unix binary as executable.
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
/// Leave executable metadata unchanged on non-Unix platforms.
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Create the Windows zip archive for a target release payload.
fn create_zip_archive(
    archive: &Path,
    payload_dir: &Path,
    payload_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(archive)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for entry in ["treeboot.exe", "README.md", "LICENSE"] {
        let source = payload_dir.join(entry);
        zip.start_file(format!("{payload_name}/{entry}"), options)?;
        let mut input = File::open(source)?;
        io::copy(&mut input, &mut zip)?;
    }

    zip.finish()?;
    Ok(())
}

/// Create the non-Windows tar.gz archive for a target release payload.
fn create_tar_archive(
    archive: &Path,
    payload_dir: &Path,
    payload_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("tar")
        .args(["-C"])
        .arg(
            payload_dir
                .parent()
                .ok_or("payload directory has no parent")?,
        )
        .args(["-czf"])
        .arg(archive)
        .arg(payload_name)
        .status()?;
    if !status.success() {
        return Err(format!("tar failed with status {status}").into());
    }

    Ok(())
}

/// Return release notes for a version, falling back when no changelog exists.
fn release_notes(changelog: &Path, version: &str) -> Result<String, Box<dyn std::error::Error>> {
    if !changelog.is_file() {
        return Ok(fallback_notes(version));
    }

    let markdown = fs::read_to_string(changelog)?;
    extract_release_notes(&markdown, version).map_or_else(
        || Ok(fallback_notes(version)),
        |notes| Ok(normalize_notes(notes)),
    )
}

/// Extract the body under a matching level-2 changelog heading.
fn extract_release_notes<'a>(markdown: &'a str, version: &str) -> Option<&'a str> {
    let root = to_mdast(markdown, &ParseOptions::default()).ok()?;
    let children = root.children()?;
    let target_version = normalize_version(version);

    for (index, node) in children.iter().enumerate() {
        let Node::Heading(heading) = node else {
            continue;
        };
        if heading.depth != 2 || normalize_heading_text(heading) != target_version {
            continue;
        }

        let start = heading.position.as_ref()?.end.offset;
        let end = children[index + 1..]
            .iter()
            .filter_map(|next| match next {
                Node::Heading(next_heading) if next_heading.depth <= heading.depth => next_heading
                    .position
                    .as_ref()
                    .map(|position| position.start.offset),
                _ => None,
            })
            .next()
            .unwrap_or(markdown.len());

        return markdown.get(start..end);
    }

    None
}

/// Trim surrounding whitespace and keep one trailing newline for release notes.
fn normalize_notes(notes: &str) -> String {
    let trimmed = notes.trim_matches(|character: char| character.is_whitespace());
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

/// Build default release notes when the changelog has no matching section.
fn fallback_notes(version: &str) -> String {
    format!("Release v{}\n", normalize_version(version))
}

/// Normalize a Markdown heading node into a comparable version string.
fn normalize_heading_text(heading: &Heading) -> String {
    let text = plain_text(&heading.children);
    normalize_version_heading(&text)
}

/// Extract and normalize the version token from a changelog heading.
fn normalize_version_heading(text: &str) -> String {
    let text = text.trim();
    let text = text.strip_prefix('[').unwrap_or(text);
    let text = text.split(']').next().unwrap_or(text);
    let text = text.split_whitespace().next().unwrap_or(text);
    normalize_version(text)
}

/// Normalize a version by trimming whitespace and a leading v prefix.
fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_owned()
}

/// Flatten a sequence of phrasing Markdown nodes into plain text.
fn plain_text(nodes: &[Node]) -> String {
    let mut output = String::new();
    for node in nodes {
        append_plain_text(node, &mut output);
    }
    output
}

/// Append supported text-bearing Markdown nodes to a plain-text buffer.
fn append_plain_text(node: &Node, output: &mut String) {
    match node {
        Node::Text(text) => output.push_str(&text.value),
        Node::InlineCode(code) => output.push_str(&code.value),
        Node::Emphasis(node) => output.push_str(&plain_text(&node.children)),
        Node::Strong(node) => output.push_str(&plain_text(&node.children)),
        Node::Delete(node) => output.push_str(&plain_text(&node.children)),
        Node::Link(node) => output.push_str(&plain_text(&node.children)),
        Node::LinkReference(node) => output.push_str(&plain_text(&node.children)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read as _;

    /// Explicit version overrides accept a leading v but do not mark a tag run.
    #[test]
    fn normalizes_explicit_version() {
        assert_eq!(
            resolve_version(Some("v1.2.3"), None, None).unwrap(),
            VersionInfo {
                tag: "v1.2.3".to_owned(),
                version: "1.2.3".to_owned(),
                safe_version: "1.2.3".to_owned(),
                is_tag: false,
            }
        );
    }

    /// GitHub tag refs resolve to a tag release when the ref is vX.Y.Z.
    #[test]
    fn detects_github_tag_version() {
        assert_eq!(
            resolve_version(None, Some("tag"), Some("v1.2.3")).unwrap(),
            VersionInfo {
                tag: "v1.2.3".to_owned(),
                version: "1.2.3".to_owned(),
                safe_version: "1.2.3".to_owned(),
                is_tag: true,
            }
        );
    }

    /// GitHub tag events fail when GitHub did not provide a ref name.
    #[test]
    fn rejects_missing_github_tag_ref_name() {
        let error = resolve_version(None, Some("tag"), None)
            .unwrap_err()
            .to_string();

        assert_eq!(error, "missing github ref name for tag event");
    }

    /// GitHub tag events fail when the ref does not start with v.
    #[test]
    fn rejects_github_tag_without_v_prefix() {
        let error = resolve_version(None, Some("tag"), Some("1.2.3"))
            .unwrap_err()
            .to_string();

        assert_eq!(error, "invalid release tag '1.2.3', expected vX.Y.Z");
    }

    /// GitHub tag events fail when the ref is not strict vX.Y.Z.
    #[test]
    fn rejects_non_strict_github_tag_version() {
        let error = resolve_version(None, Some("tag"), Some("v1.2.3-beta.1"))
            .unwrap_err()
            .to_string();

        assert_eq!(
            error,
            "invalid release tag 'v1.2.3-beta.1', expected vX.Y.Z"
        );
    }

    /// Strict release versions require exactly three numeric segments.
    #[test]
    fn validates_strict_release_versions() {
        assert!(is_strict_release_version("1.2.3"));
        assert!(!is_strict_release_version("1.2"));
        assert!(!is_strict_release_version("1.2.3.4"));
        assert!(!is_strict_release_version("1.2.beta"));
        assert!(!is_strict_release_version("1..3"));
    }

    /// Artifact names replace characters that are unsafe in workflow artifacts.
    #[test]
    fn safe_version_replaces_artifact_unsafe_characters() {
        assert_eq!(
            version_info("v1.2.3".to_owned(), "1.2.3+dirty build".to_owned(), false).safe_version,
            "1.2.3-dirty-build"
        );
    }

    /// Git-derived non-tag runs use the full described version as their tag.
    #[test]
    fn derived_non_tag_version_uses_full_version_as_tag() {
        let version = resolve_version(None, None, None).unwrap();

        assert_eq!(version.tag, format!("v{}", version.version));
        assert!(!version.is_tag);
    }

    /// Version metadata appends key-value pairs to GitHub output files.
    #[test]
    fn writes_github_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("outputs");

        write_version_outputs(
            &VersionInfo {
                tag: "v1.2.3".to_owned(),
                version: "1.2.3".to_owned(),
                safe_version: "1.2.3".to_owned(),
                is_tag: true,
            },
            &output,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(output).unwrap(),
            "tag=v1.2.3\nversion=1.2.3\nsafe_version=1.2.3\nis_tag=true\n"
        );
    }

    /// Packaging creates raw and tar.gz assets for Unix-style targets.
    #[test]
    fn packages_tar_release_assets() {
        let project = fixture_project("x86_64-unknown-linux-musl", "");
        let dist_dir = project.path().join("dist");

        package_release_asset_in_project(
            project.path(),
            "x86_64-unknown-linux-musl",
            "1.2.3",
            &dist_dir,
        )
        .unwrap();

        assert_eq!(
            fs::read(dist_dir.join("treeboot-x86_64-unknown-linux-musl")).unwrap(),
            b"binary"
        );

        let output = Command::new("tar")
            .arg("-tzf")
            .arg(dist_dir.join("treeboot-x86_64-unknown-linux-musl.tar.gz"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let entries = String::from_utf8(output.stdout).unwrap();
        assert!(entries.contains("treeboot-1.2.3-x86_64-unknown-linux-musl/treeboot"));
        assert!(entries.contains("treeboot-1.2.3-x86_64-unknown-linux-musl/README.md"));
        assert!(entries.contains("treeboot-1.2.3-x86_64-unknown-linux-musl/LICENSE"));
    }

    /// Packaging creates raw and zip assets for Windows targets.
    #[test]
    fn packages_zip_release_assets() {
        let project = fixture_project("x86_64-pc-windows-msvc", ".exe");
        let dist_dir = project.path().join("dist");

        package_release_asset_in_project(
            project.path(),
            "x86_64-pc-windows-msvc",
            "1.2.3",
            &dist_dir,
        )
        .unwrap();

        assert_eq!(
            fs::read(dist_dir.join("treeboot-x86_64-pc-windows-msvc.exe")).unwrap(),
            b"binary"
        );

        let archive = File::open(dist_dir.join("treeboot-x86_64-pc-windows-msvc.zip")).unwrap();
        let mut zip = zip::ZipArchive::new(archive).unwrap();
        let mut names = zip.file_names().map(str::to_owned).collect::<Vec<String>>();
        names.sort();
        assert_eq!(
            names,
            [
                "treeboot-1.2.3-x86_64-pc-windows-msvc/LICENSE",
                "treeboot-1.2.3-x86_64-pc-windows-msvc/README.md",
                "treeboot-1.2.3-x86_64-pc-windows-msvc/treeboot.exe",
            ]
        );

        let mut binary = String::new();
        zip.by_name("treeboot-1.2.3-x86_64-pc-windows-msvc/treeboot.exe")
            .unwrap()
            .read_to_string(&mut binary)
            .unwrap();
        assert_eq!(binary, "binary");
    }

    /// Package fixtures mirror the release workflow's expected project layout.
    fn fixture_project(target: &str, exe_suffix: &str) -> tempfile::TempDir {
        let project = tempfile::tempdir().unwrap();
        fs::write(project.path().join("README.md"), "readme").unwrap();
        fs::write(project.path().join("LICENSE"), "license").unwrap();

        let binary_dir = project.path().join(format!("target/{target}/release"));
        fs::create_dir_all(&binary_dir).unwrap();
        fs::write(binary_dir.join(format!("treeboot{exe_suffix}")), "binary").unwrap();

        project
    }

    /// Changelog extraction preserves Markdown inside a linked release section.
    #[test]
    fn extracts_matching_release_section() {
        let changelog = "\
# Changelog

## [v1.2.3](https://example.test/releases/tag/v1.2.3) (2026-06-21)

### Features

- Keep **formatting** intact.

```text
## not a heading
```

## [v1.2.2] (2026-06-20)

- Old note.
";

        assert_eq!(
            extract_release_notes(changelog, "1.2.3").map(normalize_notes),
            Some(
                "\
### Features

- Keep **formatting** intact.

```text
## not a heading
```
"
                .to_owned()
            )
        );
    }

    /// Markdown offsets remain valid when Unicode appears before the heading.
    #[test]
    fn extracts_release_section_after_unicode_content() {
        let changelog = "\
# Changelog

Intro with unicode: cafe\u{301}

## v1.2.3

- Note.
";

        assert_eq!(
            extract_release_notes(changelog, "1.2.3").map(normalize_notes),
            Some("- Note.\n".to_owned())
        );
    }

    /// Plain headings match version inputs with or without a leading v.
    #[test]
    fn extracts_unlinked_version_heading() {
        let changelog = "\
# Changelog

## 1.2.3

- Note.
";

        assert_eq!(
            extract_release_notes(changelog, "v1.2.3").map(normalize_notes),
            Some("- Note.\n".to_owned())
        );
    }

    /// Extraction stops at a same-or-higher-level heading.
    #[test]
    fn stops_at_higher_heading() {
        let changelog = "\
# Changelog

## v1.2.3

- Note.

# Other

- Not part of the release.
";

        assert_eq!(
            extract_release_notes(changelog, "1.2.3").map(normalize_notes),
            Some("- Note.\n".to_owned())
        );
    }

    /// Missing changelog sections return no extracted body.
    #[test]
    fn returns_none_when_version_is_missing() {
        assert_eq!(extract_release_notes("# Changelog\n", "1.2.3"), None);
    }

    /// Missing changelog files produce fallback release notes.
    #[test]
    fn release_notes_falls_back_when_changelog_is_missing() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            release_notes(&dir.path().join("missing.md"), "v1.2.3").unwrap(),
            "Release v1.2.3\n"
        );
    }
}

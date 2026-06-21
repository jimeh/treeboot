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

#[derive(Debug, Parser)]
#[command(name = "treeboot-release-helper")]
#[command(about = "Release workflow helper for treeboot")]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

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

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

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

#[derive(Debug, Eq, PartialEq)]
struct VersionInfo {
    tag: String,
    version: String,
    safe_version: String,
    is_tag: bool,
}

fn resolve_version(
    override_version: Option<&str>,
    github_ref_type: Option<&str>,
    github_ref_name: Option<&str>,
) -> Result<VersionInfo, Box<dyn std::error::Error>> {
    if let Some(version) = override_version.filter(|value| !value.is_empty()) {
        let version = version.trim_start_matches('v').to_owned();
        return Ok(version_info(format!("v{version}"), version, false));
    }

    if github_ref_type == Some("tag")
        && let Some(ref_name) = github_ref_name
        && ref_name.starts_with('v')
    {
        let tag = ref_name.to_owned();
        let version = tag.trim_start_matches('v').to_owned();
        return Ok(version_info(tag, version, true));
    }

    let describe = git_output([
        "describe", "--tags", "--dirty", "--always", "--match", "v[0-9]*",
    ])
    .or_else(|_| git_output(["rev-parse", "--short", "HEAD"]))?;
    let version = describe.trim_start_matches('v').to_owned();
    let tag = format!("v{version}");

    Ok(version_info(tag, version, false))
}

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

fn package_release_asset(
    target: &str,
    version: &str,
    dist_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let windows = target.contains("windows");
    let exe_suffix = if windows { ".exe" } else { "" };
    let binary = PathBuf::from(format!("target/{target}/release/treeboot{exe_suffix}"));
    if !binary.is_file() {
        return Err(format!("missing release binary: {}", binary.display()).into());
    }

    fs::create_dir_all(dist_dir)?;
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
    fs::copy("README.md", payload_dir.join("README.md"))?;
    fs::copy("LICENSE", payload_dir.join("LICENSE"))?;

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

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

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

fn normalize_notes(notes: &str) -> String {
    let trimmed = notes.trim_matches(|character: char| character.is_whitespace());
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

fn fallback_notes(version: &str) -> String {
    format!("Release v{}\n", normalize_version(version))
}

fn normalize_heading_text(heading: &Heading) -> String {
    let text = plain_text(&heading.children);
    normalize_version_heading(&text)
}

fn normalize_version_heading(text: &str) -> String {
    let text = text.trim();
    let text = text.strip_prefix('[').unwrap_or(text);
    let text = text.split(']').next().unwrap_or(text);
    let text = text.split_whitespace().next().unwrap_or(text);
    normalize_version(text)
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_owned()
}

fn plain_text(nodes: &[Node]) -> String {
    let mut output = String::new();
    for node in nodes {
        append_plain_text(node, &mut output);
    }
    output
}

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

    #[test]
    fn safe_version_replaces_artifact_unsafe_characters() {
        assert_eq!(
            version_info("v1.2.3".to_owned(), "1.2.3+dirty build".to_owned(), false).safe_version,
            "1.2.3-dirty-build"
        );
    }

    #[test]
    fn derived_non_tag_version_uses_full_version_as_tag() {
        let version = resolve_version(None, None, None).unwrap();

        assert_eq!(version.tag, format!("v{}", version.version));
        assert!(!version.is_tag);
    }

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

    #[test]
    fn returns_none_when_version_is_missing() {
        assert_eq!(extract_release_notes("# Changelog\n", "1.2.3"), None);
    }

    #[test]
    fn release_notes_falls_back_when_changelog_is_missing() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            release_notes(&dir.path().join("missing.md"), "v1.2.3").unwrap(),
            "Release v1.2.3\n"
        );
    }
}

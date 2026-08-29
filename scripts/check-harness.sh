#!/usr/bin/env bash
set -euo pipefail

failures=0

fail() {
  printf 'treeboot harness: %s\n' "$*" >&2
  failures=$((failures + 1))
}

extract_readme_spec() {
  sed -nE 's/.*spec v([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' "$@" |
    head -n 1
}

extract_spec_version() {
  sed -nE 's/.*Specification v([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' "$@" |
    head -n 1
}

extract_package_version() {
  sed -nE 's/^version = "([0-9]+\.[0-9]+\.[0-9]+)"/\1/p' "$@" |
    head -n 1
}

git_path_exists() {
  git cat-file -e "$1:$2" 2>/dev/null
}

normalize_legacy_spec_relocation() {
  sed 's#schemas/treeboot\.schema\.json#crates/treeboot-spec/assets/treeboot.schema.json#g' |
    awk -v schema_path='crates/treeboot-spec/assets/treeboot.schema.json' '
      BEGIN { RS = ""; ORS = "\n\n" }
      index($0, schema_path) { gsub(/\n/, " ") }
      { print }
    '
}

version_greater_than() {
  local current="$1"
  local base="$2"
  local current_major current_minor current_patch
  local base_major base_minor base_patch

  IFS=. read -r current_major current_minor current_patch <<<"${current}"
  IFS=. read -r base_major base_minor base_patch <<<"${base}"

  if ((current_major != base_major)); then
    ((current_major > base_major))
    return
  fi

  if ((current_minor != base_minor)); then
    ((current_minor > base_minor))
    return
  fi

  ((current_patch > base_patch))
}

resolve_spec_base_ref() {
  if [[ -n "${TREEBOOT_SPEC_BASE_REF:-}" ]]; then
    printf '%s\n' "${TREEBOOT_SPEC_BASE_REF}"
    return 0
  fi

  if [[ "${GITHUB_EVENT_NAME:-}" != pull_request* ||
    -z "${GITHUB_BASE_REF:-}" ]]; then
    return 1
  fi

  local base_ref="refs/remotes/origin/${GITHUB_BASE_REF}"
  if ! git rev-parse --verify --quiet "${base_ref}" >/dev/null; then
    git fetch --no-tags --depth=1 origin "${GITHUB_BASE_REF}:${base_ref}"
  fi

  printf '%s\n' "${base_ref}"
}

readme_spec="$(
  extract_readme_spec README.md
)"
spec_version="$(
  extract_spec_version crates/treeboot-spec/SPEC.md
)"
package_version="$(
  extract_package_version crates/treeboot/Cargo.toml
)"
spec_package_version="$(
  extract_package_version crates/treeboot-spec/Cargo.toml
)"
embedded_spec_version="$(
  sed -nE 's/^pub const SPEC_VERSION: &str = "([0-9]+\.[0-9]+\.[0-9]+)";/\1/p' \
    crates/treeboot-spec/src/lib.rs
)"
mise_cargo_audit_version="$(
  sed -nE 's/^tools\."cargo:cargo-audit" = "([0-9]+\.[0-9]+\.[0-9]+)"/\1/p' \
    mise.toml | sort -u
)"
ci_cargo_audit_version="$(
  sed -nE 's/^[[:space:]]+tool: cargo-audit@([0-9]+\.[0-9]+\.[0-9]+)$/\1/p' \
    .github/workflows/ci.yml
)"

if [[ -z "${readme_spec}" ]]; then
  fail "README.md must mention the current spec version as 'spec vX.Y.Z'"
fi

if [[ -z "${spec_version}" ]]; then
  fail "crates/treeboot-spec/SPEC.md must mention the current spec version as 'Specification vX.Y.Z'"
fi

if [[ -n "${readme_spec}" && -n "${spec_version}" && "${readme_spec}" != "${spec_version}" ]]; then
  fail "README.md spec v${readme_spec} does not match crates/treeboot-spec/SPEC.md v${spec_version}"
fi

for implementation_heading in \
  "Public library compatibility" \
  "Distribution: Install and releases" \
  "Verification: Testing strategy"; do
  if grep -Fqx "## ${implementation_heading}" crates/treeboot-spec/SPEC.md; then
    fail "crates/treeboot-spec/SPEC.md must not own implementation-specific '${implementation_heading}' guidance"
  fi
done

for implementation_phrase in \
  "Rust executable" \
  "generated from the Rust schema model"; do
  if grep -Fq "${implementation_phrase}" crates/treeboot-spec/SPEC.md; then
    fail "crates/treeboot-spec/SPEC.md must remain language-agnostic; found '${implementation_phrase}'"
  fi
done

if [[ -z "${package_version}" ]]; then
  fail "crates/treeboot/Cargo.toml must expose package version X.Y.Z"
fi

if [[ "${spec_package_version}" != "${package_version}" ]]; then
  fail "treeboot-spec package v${spec_package_version} must match treeboot v${package_version}"
fi

if [[ "${embedded_spec_version}" != "${spec_version}" ]]; then
  fail "treeboot-spec SPEC_VERSION v${embedded_spec_version} must match canonical spec v${spec_version}"
fi

if [[ -z "${mise_cargo_audit_version}" || "${mise_cargo_audit_version}" == *$'\n'* ]]; then
  fail "mise.toml must use one cargo-audit version"
elif [[ -z "${ci_cargo_audit_version}" ]]; then
  fail ".github/workflows/ci.yml must pin cargo-audit"
elif [[ "${mise_cargo_audit_version}" != "${ci_cargo_audit_version}" ]]; then
  fail "CI cargo-audit v${ci_cargo_audit_version} must match mise v${mise_cargo_audit_version}"
fi

for crate_license in crates/treeboot/LICENSE crates/treeboot-core/LICENSE crates/treeboot-spec/LICENSE; do
  if ! cmp -s LICENSE "${crate_license}"; then
    fail "${crate_license} must match root LICENSE"
  fi
done

spec_base_ref="$(resolve_spec_base_ref || true)"
if [[ -n "${spec_base_ref}" ]]; then
  if ! git rev-parse --verify --quiet "${spec_base_ref}" >/dev/null; then
    fail "spec version base ref '${spec_base_ref}' is not available"
  else
    base_spec_path=""
    for candidate in crates/treeboot-spec/SPEC.md docs/SPEC.md docs/SPEC.html; do
      if git_path_exists "${spec_base_ref}" "${candidate}"; then
        base_spec_path="${candidate}"
        break
      fi
    done

    if [[ -z "${base_spec_path}" ]]; then
      fail "base ref '${spec_base_ref}' has no canonical or legacy spec document"
    elif [[ "${base_spec_path}" == "docs/SPEC.md" ]] &&
      cmp -s \
        <(normalize_legacy_spec_relocation <crates/treeboot-spec/SPEC.md) \
        <(git show "${spec_base_ref}:${base_spec_path}" |
          normalize_legacy_spec_relocation); then
      :
    elif [[ "${base_spec_path}" != "docs/SPEC.md" ]] &&
      cmp -s crates/treeboot-spec/SPEC.md \
        <(git show "${spec_base_ref}:${base_spec_path}"); then
      :
    else
      base_spec="$(git show "${spec_base_ref}:${base_spec_path}" |
        extract_spec_version)"
      if [[ -z "${base_spec}" ]]; then
        fail "base spec '${base_spec_path}' must mention 'Specification vX.Y.Z'"
      elif [[ -z "${spec_version}" ]]; then
        :
      elif ! version_greater_than "${spec_version}" "${base_spec}"; then
        fail "crates/treeboot-spec/SPEC.md differs from '${base_spec_path}' without a strictly greater spec version"
        fail "base v${base_spec}, current v${spec_version}"
      fi
    fi
  fi
fi

core_tree="$(cargo tree -p treeboot-core --locked --prefix none)"
for package in clap clap_complete anyhow; do
  if printf '%s\n' "${core_tree}" | grep -Eq "^${package} v"; then
    fail "treeboot-core must not depend on CLI/error-boundary package '${package}'"
  fi
done

spec_tree="$(cargo tree -p treeboot-spec --locked --prefix none)"
for package in treeboot treeboot-core; do
  if printf '%s\n' "${spec_tree}" | grep -Eq "^${package} v"; then
    fail "treeboot-spec must not depend on reference package '${package}'"
  fi
done

if ! bash scripts/check-spec-cases.sh; then
  fail "treeboot-spec case inventory is out of sync"
fi

if ((failures > 0)); then
  exit 1
fi

printf 'treeboot harness: ok\n'

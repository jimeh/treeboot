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

extract_html_spec() {
  sed -nE 's/.*Specification v([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' "$@" |
    head -n 1
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
html_spec="$(
  extract_html_spec docs/SPEC.html
)"

if [[ -z "${readme_spec}" ]]; then
  fail "README.md must mention the current spec version as 'spec vX.Y.Z'"
fi

if [[ -z "${html_spec}" ]]; then
  fail "docs/SPEC.html must mention the current spec version as 'Specification vX.Y.Z'"
fi

if [[ -n "${readme_spec}" && -n "${html_spec}" && "${readme_spec}" != "${html_spec}" ]]; then
  fail "README.md spec v${readme_spec} does not match docs/SPEC.html v${html_spec}"
fi

for crate_license in crates/treeboot/LICENSE crates/treeboot-core/LICENSE; do
  if ! cmp -s LICENSE "${crate_license}"; then
    fail "${crate_license} must match root LICENSE"
  fi
done

spec_base_ref="$(resolve_spec_base_ref || true)"
if [[ -n "${spec_base_ref}" ]]; then
  if ! git rev-parse --verify --quiet "${spec_base_ref}" >/dev/null; then
    fail "spec version base ref '${spec_base_ref}' is not available"
  elif [[ -n "$(git diff --name-only \
    "${spec_base_ref}...HEAD" -- docs/SPEC.html)" ]]; then
    base_html_spec="$(
      git show "${spec_base_ref}:docs/SPEC.html" | extract_html_spec
    )"
    if [[ -z "${base_html_spec}" ]]; then
      fail "base docs/SPEC.html must mention 'Specification vX.Y.Z'"
    elif [[ -z "${html_spec}" ]]; then
      :
    elif ! version_greater_than "${html_spec}" "${base_html_spec}"; then
      fail "docs/SPEC.html changed without increasing spec version"
      fail "base v${base_html_spec}, current v${html_spec}"
    fi
  fi
fi

core_tree="$(cargo tree -p treeboot-core --locked --prefix none)"
for package in clap clap_complete anyhow; do
  if printf '%s\n' "${core_tree}" | grep -Eq "^${package} v"; then
    fail "treeboot-core must not depend on CLI/error-boundary package '${package}'"
  fi
done

if ((failures > 0)); then
  exit 1
fi

printf 'treeboot harness: ok\n'

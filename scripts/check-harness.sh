#!/usr/bin/env bash
set -euo pipefail

failures=0

fail() {
  printf 'treeboot harness: %s\n' "$*" >&2
  failures=$((failures + 1))
}

readme_spec="$(
  sed -nE 's/.*spec v([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' README.md |
    head -n 1
)"
html_spec="$(
  sed -nE 's/.*Specification v([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' \
    docs/SPEC.html |
    head -n 1
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

#!/usr/bin/env bash
set -euo pipefail

mode="${1:-write}"

spec_version="$(
  sed -nE 's/.*Specification v([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' \
    docs/SPEC.md |
    head -n 1
)"

if [[ -z "${spec_version}" ]]; then
  printf 'failed to determine spec version from docs/SPEC.md\n' >&2
  exit 1
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

printf '%s' "$spec_version" >"$tmp"

target="crates/treeboot-core/assets/spec-version.txt"

case "${mode}" in
  write)
    mkdir -p "$(dirname "$target")"
    cp "$tmp" "$target"
    ;;
  check)
    diff -u "$target" "$tmp"
    ;;
  *)
    printf 'usage: %s [write|check]\n' "$0" >&2
    exit 2
    ;;
esac

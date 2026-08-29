#!/usr/bin/env bash
set -euo pipefail

stats="$(sccache --show-stats)"
printf '%s\n' "${stats}"

if [[ "${SCCACHE_GHA_RW_MODE:-READ_WRITE}" == "READ_ONLY" ]]; then
  exit 0
fi

misses="$(
  awk '$1 == "Cache" && $2 == "misses" && NF == 3 { print $3 }' <<<"${stats}"
)"
write_errors="$(
  awk '$1 == "Cache" && $2 == "write" && $3 == "errors" { print $4 }' <<<"${stats}"
)"

if [[ "${misses}" =~ ^[0-9]+$ && "${write_errors}" =~ ^[0-9]+$ ]] &&
  ((misses > 0 && write_errors * 2 >= misses)); then
  printf '::warning title=sccache persistence degraded::%s of %s cache writes failed\n' \
    "${write_errors}" "${misses}"
fi

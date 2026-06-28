#!/usr/bin/env bash

set -euo pipefail

if [[ "$#" -eq 0 ]]; then
  exit 0
fi

mise exec -- oxfmt --check "$@"
mise exec -- markdownlint-cli2 "$@"

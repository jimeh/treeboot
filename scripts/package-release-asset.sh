#!/usr/bin/env bash
set -euo pipefail

cargo run --quiet -p treeboot-release-helper --locked -- package "$@"

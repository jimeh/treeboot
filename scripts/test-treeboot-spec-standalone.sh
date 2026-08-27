#!/usr/bin/env bash
set -euo pipefail

cargo build -p treeboot -p treeboot-spec --locked

executable_suffix=""
host="$(rustc -vV | sed -n 's/^host: //p')"
case "${host}" in
  *-windows-*) executable_suffix=".exe" ;;
esac

temp_dir="$(mktemp -d)"
trap 'rm -rf "${temp_dir}"' EXIT

candidate="${temp_dir}/candidate-treeboot${executable_suffix}"
suite="${temp_dir}/treeboot-spec${executable_suffix}"
cp "target/debug/treeboot${executable_suffix}" "${candidate}"
cp "target/debug/treeboot-spec${executable_suffix}" "${suite}"

"${suite}" test --profile full -- "${candidate}"

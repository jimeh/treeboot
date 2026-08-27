#!/usr/bin/env bash
set -euo pipefail

tmpdir="$(mktemp -d)"
trap 'rm -rf "${tmpdir}"' EXIT

source_tests="${tmpdir}/source-tests"
reference_tests="${tmpdir}/reference-tests"
portable_tests="${tmpdir}/portable-tests"
registry_tests="${tmpdir}/registry-tests"

for test_file in \
  check cli completions config doctor env init manual run schema status teardown \
  version worktree; do
  test_path="crates/treeboot/tests/${test_file}.rs"
  awk -v family="${test_file}" '
    /^#\[test\]$/ { test = 1; next }
    test && /^fn [a-zA-Z0-9_]+\(/ {
      name = $2
      sub(/\(.*/, "", name)
      print family "." name
      test = 0
    }
  ' "${test_path}"
done | sort >"${source_tests}"

cat >"${reference_tests}" <<'EOF'
completions.completions_should_include_current_subcommands_and_flags
completions.completions_should_omit_removed_init_script_flag
completions.dynamic_completions_should_include_manual_command_flags
completions.dynamic_completions_should_include_nested_worktree_commands_and_formats
completions.dynamic_completions_should_include_teardown_flags
completions.dynamic_identity_completions_should_suggest_directories
manual.dynamic_completion_should_list_root_relative_sources
manual.dynamic_completion_should_use_root_equals_option_for_sources
manual.dynamic_completion_should_use_root_option_for_sources
schema.schema_should_describe_both_teardown_command_forms
schema.schema_should_describe_worktree_identity_settings
EOF

comm -23 "${source_tests}" "${reference_tests}" >"${portable_tests}"

sed -nE \
  's/.*CaseMetadata::new\("[^"]+", "([^"]+)".*/\1/p' \
  crates/treeboot-spec/src/cases/generated.rs |
  sort -u >"${registry_tests}"

if [[ "$(wc -l <"${source_tests}")" -ne 313 ]]; then
  printf 'treeboot spec cases: expected 313 source tests\n' >&2
  exit 1
fi
if [[ "$(wc -l <"${reference_tests}")" -ne 11 ]]; then
  printf 'treeboot spec cases: expected 11 reference-only tests\n' >&2
  exit 1
fi
if [[ "$(wc -l <"${portable_tests}")" -ne 302 ]]; then
  printf 'treeboot spec cases: expected 302 portable source tests\n' >&2
  exit 1
fi
if ! diff -u "${portable_tests}" "${registry_tests}"; then
  printf 'treeboot spec cases: portable source and registry keys differ\n' >&2
  exit 1
fi

printf 'treeboot spec cases: 302 portable, 11 reference-only\n'

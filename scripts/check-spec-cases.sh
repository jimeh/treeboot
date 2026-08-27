#!/usr/bin/env bash
set -euo pipefail

tmpdir="$(mktemp -d)"
trap 'rm -rf "${tmpdir}"' EXIT

source_tests_raw="${tmpdir}/source-tests-raw"
source_tests="${tmpdir}/source-tests"
reference_tests_raw="${tmpdir}/reference-tests-raw"
reference_tests="${tmpdir}/reference-tests"
expected_tests_raw="${tmpdir}/expected-tests-raw"
expected_tests="${tmpdir}/expected-tests"
registry_tests_raw="${tmpdir}/registry-tests-raw"
registry_tests="${tmpdir}/registry-tests"
duplicates="${tmpdir}/duplicates"
overlap="${tmpdir}/overlap"
closure_cases="${tmpdir}/closure-cases"
exact_cases="${tmpdir}/exact-cases"

while IFS= read -r test_path; do
  family="${test_path#crates/treeboot/tests/}"
  family="${family%.rs}"
  family="${family//\//.}"
  awk -v family="${family}" '
    /^[[:space:]]*#\[test\][[:space:]]*$/ { test = 1; next }
    test && /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn [a-zA-Z0-9_]+\(/ {
      name = $0
      sub(/^[[:space:]]*/, "", name)
      sub(/^pub(\([^)]*\))?[[:space:]]+/, "", name)
      sub(/^fn[[:space:]]+/, "", name)
      sub(/\(.*/, "", name)
      print family "." name
      test = 0
    }
  ' "${test_path}"
done < <(find crates/treeboot/tests -type f -name '*.rs' | sort) >"${source_tests_raw}"
sort -u "${source_tests_raw}" >"${source_tests}"

cat >"${reference_tests_raw}" <<'EOF'
cli.reference_clap_diagnostics_should_preserve_parser_wording
cli.reference_generated_completion_scripts_should_include_complete_marker
cli.reference_help_and_versions_should_match_embedded_assets
cli.reference_output_failures_should_use_treeboot_diagnostic
completions.completions_should_include_current_subcommands_and_flags
completions.completions_should_omit_removed_init_script_flag
completions.dynamic_completions_should_include_manual_command_flags
completions.dynamic_completions_should_include_nested_worktree_commands_and_formats
completions.dynamic_completions_should_include_teardown_flags
completions.dynamic_identity_completions_should_suggest_directories
completions.installed_bash_completion_helper_should_list_root_sources
completions.installed_zsh_completion_helper_should_list_root_sources
manual.dynamic_completion_should_list_root_relative_sources
manual.dynamic_completion_should_use_root_equals_option_for_sources
manual.dynamic_completion_should_use_root_option_for_sources
schema.schema_should_describe_both_teardown_command_forms
schema.schema_should_describe_worktree_identity_settings
spec_closure.recursive_diagnostic_uses_normalized_symlinked_directory_source
EOF
sort -u "${reference_tests_raw}" >"${reference_tests}"

{
  cat "${reference_tests_raw}"
  printf '%s\n' 'conformance.official_binary_should_pass_full_conformance_suite'
} >"${expected_tests_raw}"
sort -u "${expected_tests_raw}" >"${expected_tests}"

sed -nE \
  's/.*CaseMetadata::new\("[^"]+", "([^"]+)".*/\1/p' \
  crates/treeboot-spec/src/cases/generated.rs >"${registry_tests_raw}"
sort -u "${registry_tests_raw}" >"${registry_tests}"

if [[ "$(wc -l <"${reference_tests_raw}")" -ne 18 ]] ||
  [[ "$(wc -l <"${reference_tests}")" -ne 18 ]]; then
  printf 'treeboot spec cases: reference-only allowlist must contain 18 unique keys\n' >&2
  exit 1
fi
if [[ "$(wc -l <"${expected_tests_raw}")" -ne 19 ]] ||
  [[ "$(wc -l <"${expected_tests}")" -ne 19 ]]; then
  printf 'treeboot spec cases: expected test allowlist must contain 19 unique keys\n' >&2
  exit 1
fi

sort "${source_tests_raw}" | uniq -d >"${duplicates}"
if [[ -s "${duplicates}" ]]; then
  printf 'treeboot spec cases: duplicate reference test declarations:\n' >&2
  cat "${duplicates}" >&2
  exit 1
fi
if ! diff -u "${expected_tests}" "${source_tests}"; then
  printf 'treeboot spec cases: integration tests must match 18 reference-only keys and the conformance driver\n' >&2
  printf 'portable observable behavior belongs in treeboot-spec\n' >&2
  exit 1
fi

if [[ "$(wc -l <"${registry_tests}")" -ne 302 ]]; then
  printf 'treeboot spec cases: expected 302 unique generated registry source keys\n' >&2
  printf 'found %s unique keys\n' "$(wc -l <"${registry_tests}")" >&2
  exit 1
fi

comm -12 "${source_tests}" "${registry_tests}" >"${overlap}"
if [[ -s "${overlap}" ]]; then
  printf 'treeboot spec cases: reference tests must not overlap portable registry keys:\n' >&2
  cat "${overlap}" >&2
  exit 1
fi

conformance_driver="crates/treeboot/tests/conformance.rs"
if [[ ! -f "${conformance_driver}" ]] ||
  ! grep -Fq 'fn official_binary_should_pass_full_conformance_suite()' \
    "${conformance_driver}" ||
  ! grep -Fq 'Suite::current()' "${conformance_driver}" ||
  ! grep -Fq 'profile: ConformanceProfile::Full' "${conformance_driver}" ||
  ! grep -Fq 'CARGO_BIN_EXE_treeboot' "${conformance_driver}"; then
  printf 'treeboot spec cases: official conformance driver is missing or incomplete\n' >&2
  printf 'expected %s to run Suite::current() against CARGO_BIN_EXE_treeboot\n' \
    "${conformance_driver}" >&2
  exit 1
fi

standalone_driver="scripts/test-treeboot-spec-standalone.sh"
if [[ ! -f "${standalone_driver}" ]] ||
  ! grep -Fq 'test --profile full --' "${standalone_driver}"; then
  printf 'treeboot spec cases: standalone conformance driver must request the full profile\n' >&2
  exit 1
fi

sed -nE 's/^[[:space:]]+"(closure\.[^"]+)".*/\1/p' \
  crates/treeboot-spec/src/cases/closure.rs | sort -u >"${closure_cases}"
sed -n '/^closure\.exact\./p' "${closure_cases}" >"${exact_cases}"
if [[ "$(wc -l <"${closure_cases}")" -ne 20 ]] ||
  [[ "$(wc -l <"${exact_cases}")" -ne 6 ]]; then
  printf 'treeboot spec cases: expected 20 closure cases including six exact-identity cases\n' >&2
  exit 1
fi

printf 'treeboot spec cases: 302 audited cases, 20 closure cases, 18 reference-only tests, full conformance drivers present\n'

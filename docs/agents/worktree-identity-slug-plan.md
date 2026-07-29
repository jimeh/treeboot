# Worktree ID, Slug, and Explicit-Path Plan

Status: approved for implementation.

## Objective

Split Treeboot's current readable identifier into:

- a compact opaque worktree ID containing only the configured lowercase
  Crockford Base32 digest prefix; and
- a readable worktree slug containing the sanitized name, separator, and the
  complete ID.

Extend `treeboot worktree id` and the new `treeboot worktree slug` command with
an optional path that derives values for that exact target even when it is not a
Git worktree.

The observable contract belongs in [the specification](../SPEC.md), and current
module responsibilities belong in [the architecture guide](../ARCHITECTURE.md).

## Contract

### Values and configuration

With defaults, a path produces values such as:

```text
ID:   k7m2qx
Slug: feature-login-k7m2qx
```

Configured commands receive both owned variables:

```text
TREEBOOT_WORKTREE_ID=k7m2qx
TREEBOOT_WORKTREE_SLUG=feature-login-k7m2qx
```

Replace the unreleased composite `worktree_id` presentation object with two
normalized top-level settings objects:

```toml
worktree_id = { length = 6 }
worktree_slug = { max_length = 48, separator = "-" }
```

The ID length remains configurable from 1 through 52 characters. The slug always
contains the complete ID, so `worktree_slug.max_length` must be at least
`worktree_id.length + 2`. The separator remains exactly `-` or `_`.

Keep the existing SHA-256 domain, platform-native path bytes, Crockford
alphabet, and digest-prefix behavior. The canonical target path remains the only
hash input. The slug retains the existing manager recognizers, sanitization,
truncation, and DNS-label behavior.

This is a breaking correction to the unreleased 2.3.0 contract. Bump the
specification to 3.0.0 and update the generated spec-version asset and config
schema. Do not retain aliases for the unreleased composite settings shape.

### CLI inspection

The command surface is:

```console
treeboot worktree id [PATH]
treeboot worktree slug [PATH]
treeboot worktree path <ID>
treeboot worktree list
```

Without `PATH`, `id` and `slug` preserve config-aware Git discovery for the
current worktree. Text output is the bare requested value. Structured output is
exactly `{ "id": ... }` or `{ "slug": ... }`.

With `PATH`, both commands:

1. Resolve a relative input against the process current directory.
2. Normalize to an absolute path by canonicalizing the longest existing ancestor
   and lexically normalizing any missing suffix.
3. Treat that normalized path itself as the target; never replace it with an
   enclosing Git worktree root.
4. Reject an existing target that is not a directory.
5. Work when the target is an ordinary directory, is nonexistent, or the
   invocation is outside Git.
6. Ignore ambient root and default-branch compatibility overrides.
7. Discover and fully parse config from the exact target directory when that
   directory exists; use default identity settings otherwise.
8. Preserve the current failure-on-invalid-discovered-config rule.

For an actual worktree, explicit and implicit invocation must agree. Symlink,
relative, absolute, `.` and `..` spellings that resolve to one target must
agree. A nonexistent target keeps the normalized identity when later created as
an ordinary directory under the same canonical ancestor.

Git discovery may supply the main-worktree basename only for readable fallback;
failure to discover Git is not an error in explicit-path mode. Without a Git
main-worktree fallback, use the target basename, then `worktree` if sanitization
is empty. This fallback affects only the slug, never the ID.

Mark the optional argument as a directory path for shell completion.

### Lookup, inventory, and reports

`treeboot worktree path <ID>` resolves exact IDs only and retains the complete
scan plus explicit no-match and ambiguity errors. `treeboot worktree list`
renders `ID`, `SLUG`, and `PATH`; structured entries contain exactly `id`,
`slug`, and `path`.

Repository-wide inspection continues to load each candidate's local config.
Changing one candidate's ID length may therefore change both its ID and slug.
Slug collisions remain visible but do not affect ID lookup.

`treeboot env` reports both owned variables. Config inspection reports both
effective values and both normalized settings objects in text, JSON, and YAML.
Public core reports and options must remain forward-compatible through
non-exhaustive types and default construction.

## Implementation Shape

- Refactor the pure path algorithm to derive one identity value containing an ID
  and slug from a normalized target, optional readable fallback, and checked
  settings.
- Separate single-target identity options from repository inventory options so
  an explicit path cannot accidentally affect `list` or `path`.
- Add a non-Git synthetic context for exact-target config discovery and
  normalization; keep Git discovery optional and fallback-only in that mode.
- Keep CLI parsing and rendering in the binary crate.
- Update the public core API, config schema generator and generated schema,
  README surfaces, completions, specification, architecture, and current agent
  state summary.
- Add no dependency.

## Closure Matrix

| Behavior or risk                                                     | Required evidence                                     |
| -------------------------------------------------------------------- | ----------------------------------------------------- |
| ID is exactly the configured digest prefix and slug ends with it     | Core fixed-vector and configurable-length tests       |
| ID is unaffected by readable name, separator, or slug maximum        | Core path-derivation tests plus targeted mutation     |
| Both owned variables reach bootstrap and teardown commands           | CLI environment/run/teardown integration tests        |
| Commands cannot override either owned variable                       | Bootstrap and teardown planning failure tests         |
| `id` and `slug` no-argument modes match environment values           | CLI text, JSON, and YAML tests                        |
| Explicit actual-worktree mode matches implicit mode and local config | CLI integration test                                  |
| Explicit ordinary-directory mode works outside Git                   | CLI integration test                                  |
| Relative, absolute, symlink, `.` and `..` aliases agree              | Core normalization and CLI integration tests          |
| Nonexistent target is stable after ordinary directory creation       | CLI integration test                                  |
| Existing regular-file target fails atomically                        | CLI failure test with empty stdout                    |
| Explicit mode ignores ambient root/default-branch overrides          | CLI integration test                                  |
| Invalid exact-target config fails before stdout                      | CLI failure test                                      |
| Mechanical and empty non-Git names use deterministic fallbacks       | Core derivation tests                                 |
| Native Unix bytes and Windows path encoding remain stable            | Existing fixed vectors plus platform-gated path tests |
| Lookup uses ID only and retains no-match/ambiguity handling          | Core and CLI lookup tests                             |
| Inventory includes exact ID/slug/path shapes and ordering            | Core and CLI list tests                               |
| Config parsing validates cross-object length constraints             | Core config and CLI diagnostic tests                  |
| Generated schema and spec version match the final contract           | Generation freshness and metadata tests               |
| Public API remains constructible and non-exhaustive                  | Public API tests and doctests                         |
| Help and completions expose optional directory paths and slug        | CLI help and completion tests                         |

New tests must be observed failing for the intended reason before implementation
or under a focused behavioral mutation afterward. Runner output must confirm the
new tests were collected.

## Commands

Focused implementation checks:

```sh
rtk cargo test -p treeboot-core --lib --all-features --locked worktree
rtk cargo test -p treeboot-core --test public_api --all-features --locked worktree
rtk cargo test -p treeboot --test worktree --all-features --locked
rtk cargo test -p treeboot --test env --all-features --locked
rtk cargo test -p treeboot --test config --all-features --locked
rtk cargo test -p treeboot --test run --all-features --locked worktree
rtk cargo test -p treeboot --test teardown --all-features --locked worktree
rtk cargo test -p treeboot --test completions --all-features --locked
rtk mise run generate
```

Implementer handoff checks:

```sh
rtk mise run format
rtk mise run test:core
rtk mise run test:cli
rtk mise run coverage:missing
```

Intended-final-head local gate:

```sh
rtk mise run verify
```

GitHub Actions supplies the full Linux, macOS, and Windows test matrix plus
format, generation, harness, lint, MSRV, and Actions validation. CodeRabbit is
the final repository review gate after dual review, final local validation, and
final-head CI.

## Risks

- A configurable short ID can collide; exact ambiguity detection remains
  required.
- Full target-local config parsing may reject identity inspection because of an
  unrelated invalid declaration; this deliberately preserves current behavior.
- Nonexistent paths can later gain a symlink component and canonicalize
  differently; stability is guaranteed only while the resolved target identity
  remains the same.
- Native path handling and error rendering differ by platform; CI is the
  cross-platform authority.

## Non-Goals

- Global or cross-repository lookup.
- Lookup by slug, ID prefix, branch, or commit.
- Arbitrary user-supplied IDs or slugs.
- Persisting identity independently of canonical path.
- Supporting existing regular files as hypothetical worktree roots.
- Changing the hash algorithm, alphabet, or platform-native encoding.
- Repairing or pruning Git worktree metadata.

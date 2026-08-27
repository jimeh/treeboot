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

## Settled Behavior Sources

Treat the following specification sections as the complete observable contract:

- [Worktree inspection commands](../SPEC.md#treeboot-worktree)
- [Public struct construction](../../crates/treeboot-core/README.md#public-struct-construction)
- [Worktree ID and slug](../SPEC.md#worktree-id-and-slug)

Use the [command-to-core map](../ARCHITECTURE.md#entry-points-command-surface),
[context and identity model](../ARCHITECTURE.md#environment-aliases-and-identity),
and [public struct evolution policy](../ARCHITECTURE.md#public-struct-evolution)
for current implementation placement. Update those source documents rather than
restating behavior here.

Delivery remains additive from the released specification 2.1.0 to 2.4.0.
Regenerate the spec-version and schema assets, and do not add aliases for the
intermediate main-only composite settings shape that existed only on `main`.

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
| Existing files and dangling symlinks fail atomically                 | CLI failure tests with empty stdout                   |
| Empty and unsupported Windows path forms fail before output          | CLI and public API platform-gated tests               |
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

Post-draft correction checks reuse focused core/worktree/config/completion
tests, then run `format`, `generate:check`, `test:core`, and `test:cli`.
Coverage and broad verification remain bound to the reviewed implementation head
unless a correction invalidates that evidence.

Intended-final-head local gate:

```sh
rtk mise run verify
```

GitHub Actions supplies the full Linux, macOS, and Windows test matrix plus
format, generation, harness, lint, MSRV, and Actions validation. CodeRabbit is
the final repository review gate after two independent reviews, final local
validation, and CI on the exact final commit.

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

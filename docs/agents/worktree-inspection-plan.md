# Worktree Inspection Commands Plan

Status: approved for implementation.

## Objective

Add side-effect-free commands for inspecting and resolving Treeboot worktree
identifiers:

```console
treeboot worktree id
treeboot worktree path <ID>
treeboot worktree list
```

The commands must use the same effective, candidate-local identifier settings
that configured commands receive through `TREEBOOT_WORKTREE_ID`.

## Observable Contract

### Current identifier

`treeboot worktree id` discovers and fully parses the current worktree's config
using the same rules as `treeboot env`.

- Text output is the bare identifier followed by a newline.
- JSON and YAML output contain an `id` field.

### Reverse lookup

`treeboot worktree path <ID>` enumerates the current repository's registered,
non-bare worktrees whose paths still exist. It canonicalizes every candidate,
loads that candidate's discovered config, and compares the complete effective
identifier exactly and case-sensitively.

- The main worktree participates in lookup.
- Text output is the bare canonical absolute path followed by a newline.
- JSON and YAML output contain `id` and `path`.
- No match is a typed non-zero error.
- Multiple matches are a distinct typed non-zero error that reports every
  matching canonical path.
- Lookup scans every candidate before succeeding so a later collision cannot be
  missed.

### Inventory

`treeboot worktree list` uses the same candidate discovery and identifier
generation as reverse lookup.

- Text output is an `ID` and `PATH` table.
- JSON and YAML output contain a `worktrees` array of `id` and `path` objects.
- The main worktree is first. Remaining entries are sorted by canonical path.
- Duplicate identifiers remain visible; they do not make listing fail.

All three leaf commands support `--format text|json|yaml`, `--json`, and
`--yaml`.

## Failure and Consistency Rules

- Missing registered paths are stale and skipped without pruning or otherwise
  mutating Git metadata.
- Bare repository records are excluded because they are not worktree checkouts.
- An existing candidate that cannot be canonicalized, discovered, or fully
  parsed makes the whole operation fail.
- A candidate disappearing during inspection is skipped only for a not-found
  error. Other I/O errors are fatal.
- Reports are collected completely before rendering so failures never leave
  partial stdout.
- Ambient recognized Treeboot environment input remains honored.
- The first version does not add `--root` or `--config`; their cross-candidate
  semantics are intentionally left out.

## Implementation Shape

- Add a public core inspection facade with options, entry/report types, current
  ID inspection, reverse lookup, and inventory functions.
- Keep option and report structs forward-compatible with the crate's existing
  public API policy.
- Extend Git plumbing to parse every NUL-delimited
  `git worktree list --porcelain -z` record without losing native path bytes,
  and distinguish bare records.
- Share effective config-aware identifier resolution with `treeboot env` instead
  of duplicating it.
- Add typed, path-attributed candidate inspection, no-match, and ambiguous-ID
  errors.
- Keep CLI parsing and text/structured rendering in the binary crate.
- Update the behavioral spec, architecture map, README, public crate surface,
  completions, and generated spec-version asset. This additive contract bumps
  the spec from 2.2.0 to 2.3.0.

No new dependency or config-schema change is expected.

## Test Strategy

Core coverage:

- parse multiple porcelain records, unknown fields, bare records, embedded
  whitespace/newlines, and Unix non-UTF-8 paths;
- main-first and canonical-path ordering;
- stale entry filtering and fatal non-not-found failures;
- duplicate IDs in inventory;
- unique, missing, and ambiguous reverse lookup;
- candidate failure preventing partial or falsely complete results;
- current ID matching the config-refined command environment.

CLI integration coverage:

- bare text and exact structured shapes for all three commands;
- custom identifier settings matching `treeboot env`;
- reverse lookup of main and linked worktrees;
- inventory containing main and multiple linked worktrees in contract order;
- candidate-local config differences;
- default settings when config is missing;
- malformed sibling config failure with empty stdout and candidate attribution;
- stale registered worktree filtering;
- missing and deliberately colliding IDs;
- invocation outside Git;
- platform-safe displayed paths;
- nested help, version, and completion exposure.

New tests must first fail for the absent or deliberately perturbed behavior,
then pass after restoration. Perturb config refinement and unique-match
selection independently.

Focused commands:

```sh
rtk cargo test -p treeboot-core --lib --all-features --locked worktree
rtk cargo test -p treeboot-core --test public_api --all-features --locked worktree
rtk cargo test -p treeboot --test worktree --all-features --locked
rtk cargo test -p treeboot --test completions --all-features --locked
```

Broader verification:

```sh
rtk mise run test:core
rtk mise run test:cli
rtk mise run coverage:missing
rtk mise run verify
```

## Risks

- A worktree may disappear while inventory is being built.
- Configurable short hashes can collide.
- A malformed sibling config can block repository-wide inspection.
- Git paths may contain bytes that are not UTF-8.
- Fully parsing config for every candidate is more expensive than hashing paths
  alone.

Complete-before-output behavior, explicit ambiguity errors, candidate-attributed
failures, native NUL-delimited parsing, and correctness-first config loading
address these risks.

## Non-Goals

- Cross-repository or global lookup.
- Deleted-worktree history.
- Git worktree pruning or repair.
- Bare repository identities.
- Lookup by identifier prefix.
- Branch or HEAD reporting.
- Alternate identifier algorithms.
- Ignoring malformed existing candidate config.
- Explicit cross-candidate config or source-root overrides.

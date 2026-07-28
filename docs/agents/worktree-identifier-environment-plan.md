# Worktree Identifier Environment Plan

Status: approved design; implementation pending.

This plan adds a stable, human-readable identifier for the current Git worktree.
Treeboot will expose it to every configured bootstrap and teardown command as
`TREEBOOT_WORKTREE_ID`.

The identifier is derived only from Git's resolved, canonical absolute worktree
path. Its readable portion uses a concise, stable label selected from the path
according to known worktree-manager layouts; its hash portion distinguishes
identical-looking labels in different locations.

The observable behavior in this plan must be incorporated into
[`docs/SPEC.md`](../SPEC.md) before implementation is complete. Implementation
placement and public API structure must remain consistent with
[`docs/ARCHITECTURE.md`](../ARCHITECTURE.md).

## Recommendation At A Glance

Use this default contract:

```text
TREEBOOT_WORKTREE_ID=<readable-name>-<6-lowercase-base32-characters>
```

```toml
worktree_id = { max_length = 48, hash_length = 6, separator = "-" }
```

- `max_length` is the maximum length of the complete value, including the
  separator before the hash and the hash itself.
- `hash_length` is the number of lowercase Crockford base32 characters retained
  from a SHA-256 digest.
- `separator` is used both to replace runs of unsupported name characters and to
  separate the readable portion from the hash.
- Omitting `worktree_id` uses the defaults above.
- The environment variable name, digest algorithm, and base32 alphabet are fixed
  contract, not configuration.

For an illustrative path such as `/home/alice/worktrees/payments/feature-login`,
the result will have this shape:

```text
feature-login-k7m2qx
```

The example digest is illustrative; the real suffix is computed from the full
canonical absolute path.

The default output is a DNS-compatible label: lowercase ASCII alphanumeric
characters and hyphens, starting and ending with an alphanumeric character, with
a maximum length of 48. Configuring `_` as the separator intentionally trades
that property for compatibility with unquoted SQL and programming-language
identifiers.

## Goals

- Give setup and teardown commands one short identifier suitable for local
  database names, container names, cache namespaces, DNS labels, logs, and
  similar per-worktree resources.
- Keep the value recognizable to a person while distinguishing the same readable
  name under different absolute paths.
- Produce the same value every time Treeboot resolves the same on-disk checkout,
  including after a managed worktree's Git branch changes.
- Keep the default value at or below 48 ASCII characters.
- Make the default output directly usable where DNS-label syntax is required.
- Let projects tune the overall length, hash length, and separator without
  redefining the identity algorithm.
- Make the exact effective value visible through `treeboot env`.
- Apply the same value and validation to bootstrap and teardown commands.
- Recognize stable path layouts used by common worktree managers without
  depending on mutable application state.

## Non-Goals

- Deriving identity from the current branch name, repository URL, config path,
  command phase, current process directory, task title, chat title, or vendor
  application database.
- Using the overridable file-operation source `root_path` as identity input. The
  Git-discovered main-worktree path may supply a readable fallback, but `--root`
  and root environment aliases must not change the result.
- Guaranteeing uniqueness. A truncated digest lowers collision risk but is not a
  global allocation service.
- Treating the identifier as a secret, authentication token, or
  security-sensitive hash.
- Adding compatibility aliases for other tools.
- Letting a command override `TREEBOOT_WORKTREE_ID` through its `env` table.
- Adding environment-variable overrides for the identifier settings in the first
  version.
- Exposing separate readable-name and hash variables. One value is simpler to
  carry into scripts and resource names.
- Guaranteeing DNS-label compatibility after users customize `separator` or
  increase `max_length` beyond another system's limit.

## User-Facing Contract

### Environment Variable

Configured bootstrap and teardown commands receive:

```text
TREEBOOT_WORKTREE_ID
```

It joins the existing canonical environment:

```text
TREEBOOT_ROOT_PATH
TREEBOOT_WORKTREE_PATH
TREEBOOT_WORKTREE_ID
TREEBOOT_DEFAULT_BRANCH
```

`TREEBOOT_WORKTREE_ID` is Treeboot-owned. A bootstrap or teardown command whose
`env` table declares the same name fails planning before any file operation or
command runs, matching the existing protection for `TREEBOOT_ROOT_PATH` and
other owned variables.

No alias should be added initially. The value describes a Treeboot contract, and
no existing compatibility ecosystem has a corresponding variable to mirror.

### Configuration

Add one optional top-level inline object:

```toml
worktree_id = { max_length = 48, hash_length = 6, separator = "-" }
```

All fields are optional within the object and independently default:

```toml
# Equivalent customizations:
worktree_id = { max_length = 64 }
worktree_id = { hash_length = 10 }
worktree_id = { separator = "_" }
```

The inline object is preferable to a `[worktree_id]` table in the primary
examples. In TOML, a table header remains active for following keys, so placing
`[worktree_id]` beside Treeboot's existing top-level `copy`, `commands`, and
other lists is easy to get wrong. One inline value keeps the settings grouped
without changing the scope of later declarations.

Normalized `Config` and structured `treeboot config` output always contain the
fully defaulted object:

```json
{
  "worktree_id": {
    "max_length": 48,
    "hash_length": 6,
    "separator": "-"
  }
}
```

Human-readable `treeboot config` output should show both the normalized settings
and the resulting value for the selected worktree:

```text
worktree id:
  value: payments-london-k7m2qx
  max_length: 48
  hash_length: 6
  separator: "-"
```

Do not add the setting to the minimal `treeboot init` starter file. The defaults
should require no boilerplate. Add it to the README's complete configuration
example and to the generated JSON Schema so users can discover and customize it.

### Validation

Apply defaults for every omitted field, then reject the normalized config when:

- `hash_length` is outside `1..=52`. An unpadded base32 encoding of a 256-bit
  SHA-256 digest contains 52 characters.
- `max_length` cannot hold at least one readable character, one separator, and
  the configured hash: `max_length < hash_length + 2`.
- `separator` is not exactly one ASCII `_` or `-` character.

The combined length check therefore uses defaulted values as well as explicitly
declared ones. For example, `worktree_id = { hash_length = 50 }` fails because
the default `max_length = 48` cannot hold the readable character, separator, and
hash. Attribute this combined error to the `worktree_id` declaration and include
both normalized values in the diagnostic; field-specific range and separator
errors should name the offending field at the same declaration location.

Limiting the separator to `_` or `-` keeps the result useful as a conservative
identifier across shells and common developer tools. It also avoids ambiguous
alphanumeric separators and punctuation with path, shell, or resource-name
semantics.

The default `-` makes the resulting 48-character-or-shorter identifier suitable
for DNS-label-oriented systems, including Kubernetes object names. MariaDB and
PostgreSQL accept `_` in unquoted identifiers but require quoting for names
containing `-`; projects that primarily interpolate the value into unquoted SQL
may set `separator = "_"`.

There is no separate arbitrary upper bound for `max_length`. The source string
is bounded by the selected path-derived name, and the encoded digest is bounded
at 52 characters, so a large configured maximum does not require a
correspondingly large allocation. Values above another system's limit remain the
user's explicit configuration choice.

Semantic validation errors must point to the `worktree_id` declaration's line
and column, matching existing config diagnostics.

### `treeboot env`

`treeboot env` is the inspection surface for the exact environment configured
commands will receive. It must therefore become config-aware:

```sh
treeboot env
treeboot env --config .treeboot.toml
treeboot env --json
```

Behavior:

1. Resolve the canonical worktree context as today.
2. Discover and load the selected config when one exists.
3. Use its normalized `worktree_id` settings, or defaults when no config exists.
4. Print `TREEBOOT_WORKTREE_ID` with the other Treeboot-owned variables.

Add `-c, --config <path>` with the same worktree-relative selection rules used
by `run`, `config`, `check`, and `teardown`.

This intentionally changes the current rule that `treeboot env` never parses
config. An invalid discovered or explicitly selected config must now make
`treeboot env` fail, because commands would not receive an environment from that
invalid setup contract. Continuing to show a default identifier while configured
commands would use a custom one would make the inspection command misleading.

Missing discovered config remains successful and uses defaults. A missing
explicit `--config` remains an error.

## Identifier Algorithm

The algorithm is a versioned compatibility contract and should live in one small
core module rather than being reconstructed in CLI presentation code.

### Identity Inputs

- Use `Worktree.worktree_path` after the existing Git discovery and
  `normalize_existing_path` flow.
- Use the private Git-discovered `main_worktree_path` only as a readable-name
  fallback.
- Do not use the optional file-operation source `root_path`. `--root` and root
  environment aliases must not change worktree identity.
- Do not use the caller's original `cwd`. Invocations from different
  subdirectories of the same worktree must agree.
- Do not use the current Git branch. Managed worktrees may begin detached or
  rename their branch after bootstrap, and teardown must retain the bootstrap
  identifier.
- Do not use config values as hash input. Configuration changes presentation
  length and separator, not the underlying path identity.

### Stable Hash Input And Encoding

Hash an exact domain-separated, platform-native byte sequence:

```text
<ASCII "treeboot-worktree-id-v1"><0x00><ASCII platform><0x00><canonical path bytes>
```

- On Unix, hash the literal prefix `b"treeboot-worktree-id-v1\0unix\0"` followed
  immediately by the raw `OsStr` byte sequence.
- On Windows, hash the literal prefix `b"treeboot-worktree-id-v1\0windows\0"`
  followed immediately by the `OsStr` UTF-16 code units serialized in
  little-endian order.
- Compute SHA-256.
- Treat the 256-bit digest as a big-endian bitstream.
- Encode it without padding using this lowercase Crockford base32 alphabet:

  ```text
  0123456789abcdefghjkmnpqrstvwxyz
  ```

- Consume the digest from its most-significant bit in five-bit groups. For the
  final incomplete group, left-align the remaining high bit and fill the low
  four bits with zero. Emit that final character without `=` padding, producing
  exactly 52 characters for the full 256-bit digest.
- Take the first `hash_length` encoded characters. The default first six
  characters carry 30 bits of digest information.

Hashing native path data preserves identity for Unix paths that are not valid
UTF-8. Domain separation leaves room for a deliberate future algorithm version
without silently conflating old and new inputs.

Use the well-maintained RustCrypto `sha2` crate in `treeboot-core`. A stable
identifier must not use `DefaultHasher`, whose algorithm is not a compatibility
guarantee. The Crockford encoding is small enough to implement directly over the
digest bytes unless dependency review finds an already-approved encoding crate
with a smaller maintenance burden.

### Readable Name Resolution

Select a concise label from stable components of the canonical worktree path.
Recognizers may use the Git-discovered main-worktree basename as project
context, but they must never use the mutable branch name, ambient vendor
environment variables, or application databases.

Apply recognizers in this order:

| Manager       | Exact trailing component pattern                               | Readable source              | Example                  |
| ------------- | -------------------------------------------------------------- | ---------------------------- | ------------------------ |
| Codex/ChatGPT | `.../.codex/worktrees/<opaque>/<project>`                      | `<project>`                  | `treeboot-k7m2qx`        |
| Claude Code   | `.../<project>/.claude/worktrees/<name>`                       | `<name>`                     | `feature-auth-k7m2qx`    |
| T3 Code       | `.../.t3/worktrees/<project>/t3code-<opaque>`                  | `<project>`                  | `treeboot-k7m2qx`        |
| Conductor     | `.../conductor/workspaces/<project>/<city>`                    | `<project>` plus `<city>`    | `payments-london-k7m2qx` |
| Superset      | `.../.superset/worktrees/<project>/<workspace>/<workspace...>` | Components after `<project>` | `owner-feature-x-k7m2qx` |
| Generic       | No preceding pattern matches                                   | Worktree basename            | `feature-auth-k7m2qx`    |

Here `...` consumes any number of leading components, while
`<workspace>/<workspace...>` means one or more components after the Superset
project. Match the complete trailing sequence component-by-component with the
literal marker spellings shown above. Do not expand `~`, inspect `$HOME`,
`$CODEX_HOME`, or consult any other environment variable. A customized manager
root matches only if it retains the recognized trailing marker sequence;
otherwise it deliberately uses the generic rule.

Codex and Claude Code currently select the same final component that the generic
rule would select. Keeping explicit recognizers documents and tests those
supported layouts and fixes their precedence without granting them different
mechanical-name behavior.

After any recognizer or the generic rule selects a readable source, apply the
same narrow mechanical-name check before sanitization. A source consisting of
exactly one component is mechanical when it is:

- a canonical ASCII hexadecimal UUID in `8-4-4-4-12` form;
- an ASCII hexadecimal token of at least eight characters; or
- `t3code-` followed by a non-empty ASCII alphanumeric, `_`, or `-` token.

Use the Git-discovered main-worktree basename for a mechanical source. This
allows a generated UUID-like Claude Code name and an unknown mechanical basename
to fall back to the project name, while multi-component sources such as
Conductor's `<project>/<city>` remain intact. A meaningful user-selected name is
not discarded merely because it contains digits.

The recognized layouts and their precedence are compatibility contract. Changing
an existing match so that the same path receives a different readable name
changes the complete identifier and must be treated as a contract change, not an
incidental heuristic improvement.

### Readable Name Sanitization And Truncation

Sanitize the selected readable source:

1. Convert the selected native path component or components to display text with
   invalid native text replaced lossily. This affects only the readable portion;
   the hash still uses exact native path data.
2. Lowercase ASCII letters.
3. Retain ASCII letters and digits.
4. Replace each maximal run of every other character—including path separators,
   punctuation, whitespace, non-ASCII text, and replacement characters—with
   exactly one configured separator.
5. Trim configured separators from both ends.
6. If nothing remains, sanitize the Git-discovered main-worktree basename using
   the same rules.
7. If that also produces nothing, use `worktree` as the final defensive
   fallback.

The readable budget is:

```text
max_length - 1 - hash_length
```

If the sanitized value exceeds that budget:

1. Keep its leading characters up to the budget.
2. Trim any trailing separator exposed by truncation.
3. If truncation and trimming somehow produce an empty value, apply the same
   main-worktree-basename and final-literal fallback within the budget.

Finally return:

```text
<readable><separator><hash>
```

Because the output alphabet is ASCII, configured length is unambiguously both
the character count and byte count. With default settings, the result starts and
ends with an ASCII alphanumeric character and contains only lowercase ASCII
alphanumeric characters and hyphens.

### Expected Properties

- Moving the checkout changes the identifier.
- Entering the checkout through a symlink alias does not change it because
  discovery already canonicalizes the worktree path.
- Hash the exact native path representation returned by Treeboot's existing
  canonicalization without additional case folding. On a case-insensitive
  filesystem, differently cased input spellings are guaranteed to agree only
  when canonicalization returns the same native spelling.
- Changing `--root` or any root-path environment alias does not change it.
- Renaming, creating, checking out, or detaching a Git branch does not change
  it.
- Bootstrap and teardown receive the same value for the lifetime of one
  checkout.
- Two paths with the same readable name retain different hash suffixes.
- Changing only `max_length` or `separator` leaves the underlying digest
  unchanged.
- Shortening `hash_length` takes a prefix of the same base32 digest.
- Repeated unsupported characters create one separator, never a run of
  separators.
- The final value never exceeds `max_length`.
- Default output satisfies DNS-label character and boundary rules.

## Core Design

### Normalized Config Model

Add a public non-exhaustive `WorktreeIdConfig` value to normalized `Config`.
Provide `Default` for the documented defaults and read-only accessors or a
checked constructor as needed by public callers. Do not make callers construct
an invalid instance and defer failure until command execution.

Parse a spanned raw object so range and separator errors retain useful source
locations. The JSON Schema marker should apply the same field requirements and
numeric bounds.

This is not part of `ConfigRuntimeOptions`:

- it does not affect file-operation policy;
- it has no environment or CLI precedence layers;
- teardown must use it even though teardown intentionally does not resolve
  bootstrap runtime policy.

### Environment Construction

Split environment construction into two stages:

1. Context discovery constructs the existing path/default-branch variables and a
   default `TREEBOOT_WORKTREE_ID` using the resolved worktree and Git's
   discovered main-worktree identity.
2. Once config is loaded, a core helper inserts or replaces that entry using the
   normalized `WorktreeIdConfig`.

The helper must be idempotent and preserve every other environment entry. It
must work when the key is absent, including contexts created through the public
`Worktree::from_parts` constructor.

Manifest plan constructors should apply the manifest's identifier config
themselves to a cloned context before command validation and before storing that
context in `ActionPlan` or `TeardownPlan`. This ordering ensures the existing
owned-variable check sees `TREEBOOT_WORKTREE_ID` even when a `from_parts` caller
supplied an empty environment. It also preserves correct behavior for public
`treeboot-core` callers that use `Config::parse` and plan constructors directly,
not only callers that use the high-level `run` facade.

`Worktree::from_parts` intentionally treats its supplied `root_path` as the
main-worktree identity. For such synthetic contexts only, that value may
therefore supply the readable fallback when the selected worktree name is empty
or mechanical. It never enters the digest, which remains derived solely from
`worktree_path`.

High-level reports and prepared teardown values should return the same effective
context carried by the plan, so public callers do not observe a default
environment after commands received a configured one.

`inspect_config` and the revised `inspect_env` should use the same helper. The
CLI must never implement its own name selection, sanitization, or hash logic.

### Command Validation And Execution

No new merge layer is needed. Once the effective plan context contains
`TREEBOOT_WORKTREE_ID`:

- existing owned-variable validation rejects per-command overrides;
- the existing `.envs(&context.environment)` call exposes it to shell and direct
  commands;
- bootstrap and teardown automatically share identical behavior;
- dry runs report commands without spawning them, as today.

## Documentation And Compatibility

This is additive for existing config files but changes observable config,
environment, and inspection output. Update the contract together:

- bump `docs/SPEC.md` from v2.1.0 to v2.2.0;
- update the README's referenced spec version;
- regenerate `crates/treeboot-core/assets/spec-version.txt`;
- document the variable, exact base32 algorithm, name recognizers, sanitization,
  validation, config defaults, ownership rule, DNS/SQL separator tradeoff, and
  config-aware `treeboot env` behavior;
- add the option to the README's complete configuration example and environment
  list;
- update `docs/ARCHITECTURE.md` so environment construction includes the
  post-config identifier refinement;
- update `docs/agents/dependencies.md` with the `sha2` rationale;
- regenerate both checked-in schema copies.

Existing configs continue to parse with default settings. Older Treeboot
versions will reject the new `worktree_id` key because config deliberately uses
`deny_unknown_fields`; document this as the normal forward-version behavior.

Structured consumers of `treeboot env` and `treeboot config` receive additive
keys. Existing exact-key tests and consumers must be updated.

`treeboot env` also changes from config-independent inspection to truthful
effective inspection. A script that currently succeeds in a checkout with an
invalid discovered config will now receive an error until that config is fixed
or an explicit valid config is selected. Document this failure-mode change in
the release notes and command documentation.

The name recognizers, sanitization order, domain-separated hash input, base32
alphabet, bit order, and truncation order are observable compatibility contract.
Future changes that alter an existing path's identifier require an explicit spec
decision and migration consideration.

## Implementation Sequence

1. **Specify the contract first**
   - Add the exact algorithm, defaults, recognizers, sanitization, validation,
     environment ownership, inspection behavior, and examples to `docs/SPEC.md`.
   - Bump and regenerate spec-version metadata.
2. **Write failing algorithm and config tests**
   - Add fixed SHA-256/Crockford base32 vectors and normalization/truncation
     cases.
   - Add positive, near-miss, and mechanical-fallback fixtures for every
     recognized manager layout.
   - Add parsing/defaulting/combined-validation tests for `worktree_id`.
   - Confirm each new test is collected and fails against pre-feature code.
3. **Implement the core identifier**
   - Add `sha2` to `treeboot-core`.
   - Add a focused identifier module with platform-native hash input, base32
     encoding, readable-name recognition, and sanitization helpers.
   - Add checked normalized config types and source-attributed errors.
4. **Integrate effective environments**
   - Add the default identifier during context discovery.
   - Insert or replace it from normalized config before validation in both plan
     constructors, including `Worktree::from_parts` contexts.
   - Return effective contexts from run, teardown, and config inspection.
   - Keep the existing command execution and owned-variable checks as the shared
     enforcement point.
5. **Make environment inspection truthful**
   - Add config discovery/loading to `inspect_env`.
   - Add `--config` to the CLI command.
   - Render the new value in text, JSON, and YAML.
6. **Update config presentation and generated schema**
   - Show normalized settings and effective value in text config output.
   - Add the normalized object to structured output.
   - Regenerate and compare both schema assets.
7. **Finish user and architecture documentation**
   - Update README, architecture, dependency guidance, and public core API docs.
8. **Run focused and broad verification**
   - Use targeted tests while implementing.
   - Run generated freshness, coverage review, and the normal pre-handoff suite.

## Test Strategy

### Core Identifier Unit Tests

Cover:

- fixed canonical paths with exact SHA-256/lowercase Crockford base32 values;
- default output length and six-character base32 suffix;
- the exact alphabet, big-endian bit ordering, and final zero-filled group
  through the public checked configuration with `hash_length = 52`;
- a path shorter than the readable budget;
- a long readable name truncated from the right;
- truncation that exposes a trailing separator, which is removed;
- repeated punctuation, whitespace, and path separators collapsing to exactly
  one configured separator;
- leading and trailing unsupported characters producing no boundary separator;
- default `-` and configured `_`;
- a selected name that sanitizes empty and falls back to the Git-discovered
  main-worktree basename;
- both selected and main-worktree basenames sanitizing empty, exercising the
  final `worktree` fallback;
- identical readable names under different absolute parents producing different
  hashes;
- the same path under different max/hash settings retaining the same digest
  prefix;
- exact boundary where `max_length == hash_length + 2`;
- Git branch creation, rename, checkout, and detachment leaving the identifier
  unchanged;
- explicit `--root` and every root environment alias leaving the identifier
  unchanged;
- native canonical path spelling being hashed without an additional case-folding
  step;
- Unix non-UTF-8 path bytes affecting the hash without panicking;
- Windows native path encoding and ordinary non-verbatim path output, behind
  platform gates;
- default output matching the DNS-label character and boundary contract.

Add path fixtures for:

- Codex short opaque and UUID parent directories;
- Claude Code explicit and generated worktree names;
- T3 Code `t3code-<opaque>` directories;
- Conductor `<project>/<city>` directories;
- Superset simple and slash-separated workspace/branch paths;
- near-misses such as `.codex2/worktrees/<opaque>/<project>` and customized
  roots without a recognized marker sequence using the generic rule;
- a recognized Claude Code UUID name falling back to the main-worktree project
  name;
- unknown manual worktrees with meaningful basenames;
- unknown mechanical basenames falling back to the main-worktree project name.

### Config Unit Tests

Cover:

- omission produces all defaults;
- partial inline objects default omitted fields;
- normalized serialization includes all three settings;
- unknown fields remain rejected;
- hash lengths `0` and `53` fail;
- a partial `{ hash_length = 50 }` object fails against the default
  `max_length = 48`, with both normalized values in the diagnostic;
- insufficient `max_length` fails;
- empty, multi-character, alphanumeric, and unsupported punctuation separators
  fail;
- errors include the declaration line and column;
- existing configs without the object still normalize unchanged apart from the
  additive defaulted field.

### Plan And Execution Tests

Cover both bootstrap and teardown:

- shell commands receive the configured value;
- direct commands receive the configured value;
- both phases receive exactly the same value for one worktree;
- branch changes between bootstrap and teardown do not change the value;
- `--root` changes `TREEBOOT_ROOT_PATH` but not `TREEBOOT_WORKTREE_ID`;
- command-local `env = { TREEBOOT_WORKTREE_ID = "override" }` fails before
  bootstrap file effects or any phase command;
- public plan constructors apply custom config without relying on CLI
  orchestration;
- a `Worktree::from_parts` context with an empty environment receives the
  identifier before validation, rejects a command-local override, and carries
  the effective value into both plan types;
- returned run/prepared contexts match the effective command environment.

### CLI Integration Tests

Cover:

- `treeboot env` text, JSON, and YAML include the default variable;
- discovered custom config changes the displayed value;
- `treeboot env --config` selects the requested config;
- missing discovered config uses defaults;
- missing explicit config fails;
- invalid discovered config fails instead of printing a misleading default;
- `treeboot config` text shows settings and value;
- structured config output includes the normalized object;
- `treeboot env` and `treeboot config` produce the expected identifier when
  invoked from the root checkout;
- end-to-end bootstrap and teardown commands can write the value to disk;
- the exact-key environment assertions include the additive key;
- schema output and embedded schema contain the new object and constraints;
- representative default identifiers can be used as Docker Compose project names
  and satisfy the specified DNS-label syntax;
- configured underscore identifiers retain their expected output even though
  they intentionally do not satisfy DNS-label syntax.

### Test Quality Checks

- Add behavior tests before implementation and observe them fail for the
  intended missing behavior.
- For any test added after implementation, perturb the covered behavior, observe
  the assertion fail, restore it, and confirm a clean diff.
- Confirm new tests ran by name or count in runner output.
- Run `mise run coverage:missing` and inspect uncovered lines in the identifier,
  config, context, environment inspection, and plan integration paths.
- Add high-value reachable branch coverage; do not chase defensive platform-only
  error arms that cannot be exercised safely.

## Verification

Run focused checks during implementation:

```sh
mise run test:core
mise run test:cli
mise run generate
mise run generate:check
mise run format:check
mise run lint
```

Before handoff:

```sh
mise run coverage:missing
mise run check
cargo tree -p treeboot-core
```

Use `mise run verify` as well if implementation changes the broader harness or
generated-asset workflow beyond the expected schema/spec refresh.

## Alternatives Considered

### Always Use Only The Worktree Directory Name

Rejected as the complete rule because some managers use a final component that
is mechanical or unrelated to the task, such as `t3code-<opaque>`, while
Conductor's concise and stable identity is better represented by its project and
city pair. Basename remains the generic fallback.

### Slug The Entire Absolute Path

Rejected because it creates verbose values dominated by home and manager
directory names. The full path belongs in the hash input; only stable,
manager-relevant components belong in the readable portion.

### Use The Current Git Branch

Rejected because detached worktrees, branch creation, branch renames, and
ordinary checkouts make it unstable. Conductor, for example, begins with a
random city branch and later renames it after the task becomes clear. Bootstrap,
reruns, and teardown must retain one identifier.

### Read Vendor Application Metadata

Rejected because task titles, chat records, vendor databases, and ambient
environment hints may be unavailable when Treeboot is rerun outside the manager.
Path-only recognizers keep the identifier reproducible.

### Hash Only

Rejected because an opaque value is unpleasant in logs, database listings, and
container tooling. The concise readable label costs little and materially
improves operator experience.

### Use Six Hexadecimal Characters

Rejected because six hexadecimal characters carry only 24 bits. Six base32
characters carry 30 bits, preserving most of the collision resistance of eight
hexadecimal characters while meeting the shorter six-character default.

### Default To An Underscore Separator

Rejected because `_` prevents direct use as a DNS/Kubernetes-style label.
Hyphens require quoting in SQL identifiers, but quoting is available there;
DNS-label systems provide no equivalent escape for underscores. Projects that
prioritize unquoted SQL may configure `_`.

### Keep `treeboot env` Config-Free

Rejected because it would show a default identifier even when configured
commands receive a customized one. An inspection command should prefer truthful
effective output over preserving an internal no-config shortcut.

### Use Rust's `DefaultHasher` Or A Custom Digest

Rejected because `DefaultHasher` is not a stable compatibility contract, while a
custom digest implementation adds maintenance and collision-analysis burden.
SHA-256 is portable, familiar, and easy for another implementation to reproduce
from the spec.

## Approved Decisions

1. Environment name: `TREEBOOT_WORKTREE_ID`.
2. Config shape:
   `worktree_id = { max_length = 48, hash_length = 6, separator = "-" }`.
3. Digest: SHA-256 of versioned platform-native canonical worktree path data.
4. Encoding: unpadded lowercase Crockford base32 using a fixed alphabet and
   big-endian bit order.
5. Readable name: concise, stable path components selected by exact trailing
   manager patterns without ambient environment lookup, with basename as the
   generic default.
6. Branch names, task titles, application state, and overridable source roots
   never affect identity.
7. Sanitization: lowercase ASCII alphanumeric characters, with each maximal run
   of unsupported characters collapsed to one separator.
8. Fallback: apply one narrow mechanical-name predicate to every selected
   single-component source, then use the Git-discovered main-worktree basename
   and finally `worktree`.
9. Truncation: preserve the beginning of the readable name, trim any newly
   exposed trailing separator, then append exactly one separator and the hash.
10. `treeboot env`: discover config and add `--config` so output remains
    effective and trustworthy.
11. Separator choices: default `-`, configurable `_`; the default is
    DNS-label-compatible within its 48-character limit.
12. Recognizer and algorithm changes that alter existing identifiers are
    compatibility changes.
13. Environment refinement inserts or replaces the owned variable before command
    validation, including for public `Worktree::from_parts` contexts.
14. Path hashing uses Treeboot's canonical native representation without
    additional case folding.

These user-facing decisions are approved. Implementation may proceed when
requested.

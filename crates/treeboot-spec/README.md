# treeboot-spec

`treeboot-spec` is the executable, language-agnostic compatibility contract for
Treeboot implementations. It packages the canonical [specification](SPEC.md),
config [JSON Schema](assets/treeboot.schema.json), and black-box conformance
cases.

The suite invokes a candidate executable directly. Build the candidate once and
pass its path rather than using `cargo run` for each of the hundreds of case
invocations.

```console
treeboot-spec test -- /path/to/treeboot
treeboot-spec test --format json -- /path/to/treeboot
treeboot-spec test --filter run.sync -- /path/to/treeboot
treeboot-spec list
treeboot-spec show
treeboot-spec schema
```

Rust callers use the same registry:

```rust
use treeboot_spec::{CommandTemplate, RunOptions, Suite};

let implementation = CommandTemplate::new("/path/to/treeboot");
let report = Suite::current().run(&implementation, RunOptions::default())?;
assert!(report.passed());
# Ok::<(), treeboot_spec::SuiteError>(())
```

Custom `Runner` implementations can execute the same cases in another process
environment. A complete adapter must expose each temporary fixture filesystem to
the candidate and honor native arguments, working directories, environment
changes, stdin mode, and deadlines.

The default local runner captures output through pipes, so terminal-input cases
report an explicit capability skip. A custom adapter can provide terminal input
for those cases. Installed completion-script cases likewise run only when the
adapter reports that generated scripts can reinvoke its candidate from the
fixture host.

`treeboot-spec` does not specify Treeboot's Rust library API and has no
dependency on the official `treeboot` or `treeboot-core` crates.

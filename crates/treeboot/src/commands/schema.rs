use std::io::Write;
use std::path::PathBuf;

use clap::Args;
use treeboot_core::Error;

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct SchemaArgs {
    /// Write the schema to a file instead of stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

pub(crate) fn run_schema_command(args: SchemaArgs) -> treeboot_core::Result<()> {
    let schema = treeboot_core::config_schema_json();

    if let Some(path) = args.output {
        std::fs::write(&path, schema).map_err(|source| Error::Output { source })?;
        return Ok(());
    }

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    handle
        .write_all(schema.as_bytes())
        .map_err(|source| Error::Output { source })
}

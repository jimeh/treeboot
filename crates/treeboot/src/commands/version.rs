use clap::Args;

use super::output::{OutputArgs, ReportFormat, write_structured};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct VersionArgs {
    #[command(flatten)]
    output: OutputArgs,
}

pub(crate) fn run_version_command(args: VersionArgs) -> treeboot_core::Result<()> {
    let info = treeboot_core::version_info(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    match args.output.format() {
        ReportFormat::Text => {
            println!(
                "{} {} (spec {})",
                info.package, info.version, info.spec_version
            );
            Ok(())
        }
        format => write_structured(&info, format),
    }
}

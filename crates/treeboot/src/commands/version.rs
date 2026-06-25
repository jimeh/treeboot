use clap::Args;

use super::output::{OutputArgs, ReportFormat, write_structured};

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct VersionArgs {
    #[command(flatten)]
    output: OutputArgs,
}

pub(crate) fn run_version_command(args: VersionArgs) -> treeboot_core::Result<()> {
    let info = treeboot_core::treeboot_version_info();

    match args.output.format() {
        ReportFormat::Text => {
            print_version_text(&info);
            Ok(())
        }
        format => write_structured(&info, format),
    }
}

pub(crate) fn print_version_text(info: &treeboot_core::VersionInfo) {
    println!(
        "{} {} (spec {})",
        info.package, info.version, info.spec_version
    );
}

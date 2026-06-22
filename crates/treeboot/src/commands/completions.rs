use clap::Args;
use clap_complete::Shell;
use treeboot_core::Error;

#[derive(Debug, Args, Clone, Copy)]
pub(crate) struct CompletionsArgs {
    /// Shell to generate completions for.
    shell: Shell,
}

pub(crate) fn run_completions_command(args: CompletionsArgs) -> treeboot_core::Result<()> {
    let shells = clap_complete::env::Shells::builtins();
    let shell = shells
        .completer(&args.shell.to_string())
        .ok_or_else(|| Error::Output {
            source: std::io::Error::other(format!("unsupported shell {}", args.shell)),
        })?;
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let completer = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "treeboot".to_owned());

    shell
        .write_registration("COMPLETE", "treeboot", "treeboot", &completer, &mut handle)
        .map_err(|source| Error::Output { source })
}

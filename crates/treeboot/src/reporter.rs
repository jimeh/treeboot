use treeboot_core::{OutputEvent, Reporter};

pub(crate) struct StdoutReporter;

impl Reporter for StdoutReporter {
    fn report(&mut self, event: OutputEvent) -> std::io::Result<()> {
        println!("{}", event.message());
        Ok(())
    }
}

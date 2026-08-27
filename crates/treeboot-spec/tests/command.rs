use std::path::Path;

use treeboot_spec::CommandTemplate;

#[cfg(unix)]
#[test]
fn resolves_a_bare_candidate_before_case_working_directories_change() {
    let resolved = CommandTemplate::new("sh").resolve().unwrap();

    assert!(Path::new(resolved.program()).is_absolute());
}

#[cfg(windows)]
#[test]
fn resolves_a_bare_candidate_before_case_working_directories_change() {
    let resolved = CommandTemplate::new("cmd.exe").resolve().unwrap();

    assert!(Path::new(resolved.program()).is_absolute());
}

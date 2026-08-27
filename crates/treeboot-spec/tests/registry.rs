use std::collections::HashSet;

use treeboot_spec::Suite;

#[test]
fn current_registry_should_cover_each_portable_inventory_row_once() {
    let cases = Suite::current().cases().collect::<Vec<_>>();
    let ids = cases.iter().map(|case| case.id()).collect::<HashSet<_>>();
    let source_tests = cases
        .iter()
        .filter_map(|case| case.source_test())
        .collect::<HashSet<_>>();

    assert!(
        cases.len() > 302,
        "closure cases must extend the audited suite"
    );
    assert_eq!(ids.len(), cases.len());
    assert_eq!(source_tests.len(), 302);
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.source_test().is_some())
            .count(),
        302
    );
    assert!(cases.iter().all(|case| !case.spec_references().is_empty()));
}

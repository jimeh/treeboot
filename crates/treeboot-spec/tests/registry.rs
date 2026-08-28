use std::collections::HashSet;

use treeboot_spec::{CaseRequirement, Suite};

#[test]
fn current_registry_should_cover_each_portable_inventory_row_once() {
    let cases = Suite::current().cases().collect::<Vec<_>>();
    let ids = cases.iter().map(|case| case.id()).collect::<HashSet<_>>();
    let source_tests = cases
        .iter()
        .filter_map(|case| case.source_test())
        .collect::<HashSet<_>>();

    assert_eq!(cases.len(), 336, "316 audited cases plus 20 closure cases");
    assert_eq!(ids.len(), cases.len());
    assert_eq!(source_tests.len(), 316);
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.source_test().is_some())
            .count(),
        316
    );
    assert!(cases.iter().all(|case| !case.spec_references().is_empty()));
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.requirement() == CaseRequirement::Exact)
            .count(),
        6
    );
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.requirement() == CaseRequirement::Functional)
            .count(),
        330
    );
    let serialized = serde_json::to_value(cases[0]).unwrap();
    assert!(serialized.get("requirement").is_none());
}

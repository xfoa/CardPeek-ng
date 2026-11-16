//! Synthetic document fixture conformance tests.

use cardpeek_core::ResultDocument;

const JSON: &str = include_str!("../../../tests/fixtures/documents/atr-synthetic-001.json");

#[test]
fn native_json_fixture_round_trips_without_changes() {
    let document = ResultDocument::from_json(JSON).unwrap();
    assert_eq!(document.node_count(), 2);
    assert_eq!(document.to_json().unwrap(), JSON.trim_end());
}

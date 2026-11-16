//! CLI replay integration tests.

use assert_cmd::Command;
use predicates::str::contains;

const EXPECTED: &str = include_str!("../../../tests/fixtures/documents/atr-synthetic-001.json");

fn fixture(name: &str) -> String {
    format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn discovers_atr_analyser_without_executing_it() {
    Command::cargo_bin("cardpeek")
        .unwrap()
        .arg("discover")
        .arg(fixture("analysers"))
        .assert()
        .success()
        .stdout(contains(
            "atr\t1.0.0\tATR\tInspect the current Answer to Reset",
        ));
}

#[test]
fn replay_exports_the_expected_native_document() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("atr-result.json");
    Command::cargo_bin("cardpeek")
        .unwrap()
        .arg("replay")
        .arg(fixture("traces/atr-synthetic-001.json"))
        .arg(fixture("analysers"))
        .arg("atr")
        .arg("--output")
        .arg(&output)
        .assert()
        .success()
        .stdout(contains("Exported 2 nodes in revision 1"))
        .stderr(contains("Info: Read 2 ATR bytes"));
    assert_eq!(std::fs::read_to_string(output).unwrap(), EXPECTED);
}

#[test]
fn rejects_an_analyser_that_was_not_discovered() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("unused.json");
    Command::cargo_bin("cardpeek")
        .unwrap()
        .arg("replay")
        .arg(fixture("traces/atr-synthetic-001.json"))
        .arg(fixture("analysers"))
        .arg("missing")
        .arg("--output")
        .arg(&output)
        .assert()
        .failure()
        .stderr(contains("analyser missing was not discovered"));
    assert!(!output.exists());
}

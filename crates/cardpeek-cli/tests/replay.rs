//! CLI replay integration tests.

use assert_cmd::Command;
use predicates::str::contains;

const EXPECTED: &str = include_str!("../../../tests/fixtures/documents/atr-synthetic-001.json");
const GSM_USIM_EXPECTED: &str =
    include_str!("../../../tests/fixtures/documents/gsm-usim-synthetic-001.json");

fn fixture(name: &str) -> String {
    format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn discovers_analysers_without_executing_them() {
    Command::cargo_bin("cardpeek")
        .unwrap()
        .arg("discover")
        .arg(fixture("analysers"))
        .assert()
        .success()
        .stdout("ID        VERSION  TITLE             DESCRIPTION\natr       1.0.0    ATR               Inspect the current Answer to Reset\ngsm-usim  1.0.0    GSM SIM and USIM  Inspect GSM SIM and USIM files without authentication or writes\n");
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
fn replays_gsm_usim_files_and_preserves_read_only_behaviour() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("gsm-usim-result.json");
    Command::cargo_bin("cardpeek")
        .unwrap()
        .arg("replay")
        .arg(fixture("traces/gsm-usim-synthetic-001.json"))
        .arg(fixture("analysers"))
        .arg("gsm-usim")
        .arg("--output")
        .arg(&output)
        .assert()
        .success()
        .stdout(contains("Exported 63 nodes in revision 1"))
        .stderr(contains("Info: Selected 9 files and read 1 records"));
    let actual = std::fs::read_to_string(output).unwrap();
    assert_eq!(actual, GSM_USIM_EXPECTED);
    assert!(actual.contains("Text:\\nHello"));
    assert!(actual.contains("Sensitive content not read"));
    assert!(!actual.contains("VERIFY"));
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

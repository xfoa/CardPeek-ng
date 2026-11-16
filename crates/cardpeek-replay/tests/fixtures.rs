//! Synthetic trace fixture conformance tests.

use cardpeek_core::{StatusWord, TraceDocument};
use cardpeek_replay::ReplaySession;

const TRACE: &str = include_str!("../../../tests/fixtures/traces/atr-synthetic-001.json");

#[test]
fn recorded_fixture_replays_without_hardware() {
    let trace = TraceDocument::from_json(TRACE).unwrap();
    assert_eq!(trace.to_json().unwrap(), TRACE.trim_end());

    let mut session = ReplaySession::new(trace);
    assert_eq!(session.connect().unwrap().as_bytes(), [0x3b, 0]);
    let select = session.transmit(&[0, 0xa4, 4, 0, 2, 0xa0, 0]).unwrap();
    assert_eq!(select.status, StatusWord::NORMAL_PROCESSING);
    assert_eq!(select.data, [1]);
    assert_eq!(session.transmit(&[0, 0xc0, 0, 0, 0]).unwrap().data, [2, 3]);
    assert_eq!(session.warm_reset().unwrap().as_bytes(), [0x3b, 1]);
    assert_eq!(
        session
            .transmit(&[0, 0xa4, 4, 0, 2, 0xa0, 0])
            .unwrap()
            .status
            .value(),
        0x6a82
    );
}

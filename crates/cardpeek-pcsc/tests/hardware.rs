//! Opt-in integration tests for real PC/SC readers and test cards.

use std::{ffi::CString, thread, time::Duration};

use cardpeek_core::{ByteString, Protocol, ResetMode};
use cardpeek_pcsc::{ConnectionMode, PcscContext, PcscErrorKind};
use cardpeek_transport::CardSession;

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this hardware test"))
}

fn command(name: &str) -> Vec<u8> {
    let encoded = required_env(name);
    ByteString::from_hex(8, &encoded)
        .unwrap()
        .as_bytes()
        .unwrap()
        .to_vec()
}

#[test]
#[ignore = "requires a PC/SC reader, a test card, and CARDPEEK_PCSC_READER"]
fn session_lifecycle_reset_and_short_apdu() {
    let reader = required_env("CARDPEEK_PCSC_READER");
    let context = PcscContext::establish().unwrap();
    let readers = context.list_readers().unwrap();
    let selected = readers
        .iter()
        .find(|candidate| candidate.name == reader)
        .unwrap();
    assert_eq!(selected.card_present, Some(true));

    let mut session = context.connect(&reader, ConnectionMode::Shared).unwrap();
    assert!(matches!(session.protocol(), Protocol::T0 | Protocol::T1));
    assert!(session.capabilities().extended_apdus);
    assert_ne!(session.atr().unwrap().as_bytes(), &[] as &[u8]);

    let cold_atr = session.reset(ResetMode::Cold).unwrap();
    assert_ne!(cold_atr.as_bytes(), &[] as &[u8]);
    let warm_atr = session.reset(ResetMode::Warm).unwrap();
    assert_ne!(warm_atr.as_bytes(), &[] as &[u8]);

    let short_command =
        std::env::var("CARDPEEK_PCSC_SHORT_APDU").unwrap_or_else(|_| "00A40000023F00".to_owned());
    let short_command = ByteString::from_hex(8, &short_command)
        .unwrap()
        .as_bytes()
        .unwrap()
        .to_vec();
    let response = session.transmit(&short_command).unwrap();
    assert!(response.encode().len() >= 2);

    session.disconnect().unwrap();
}

#[test]
#[ignore = "requires CARDPEEK_PCSC_READER and a safe CARDPEEK_PCSC_EXTENDED_APDU"]
fn extended_apdu() {
    let reader = required_env("CARDPEEK_PCSC_READER");
    let command = command("CARDPEEK_PCSC_EXTENDED_APDU");
    let context = PcscContext::establish().unwrap();
    let mut session = context.connect(&reader, ConnectionMode::Shared).unwrap();
    let response = session.transmit(&command).unwrap();
    assert!(response.encode().len() >= 2);
    session.disconnect().unwrap();
}

#[test]
#[ignore = "requires CARDPEEK_PCSC_EMPTY_READER with no card inserted"]
fn cancels_card_wait_and_cleans_up() {
    let reader = required_env("CARDPEEK_PCSC_EMPTY_READER");
    let reader = CString::new(reader).unwrap();
    let context = PcscContext::establish().unwrap();
    let cancellation = context.cancellation_handle();
    let token = cancellation.token().clone();

    let worker = thread::spawn(move || {
        context.wait_for_card(&reader, Some(Duration::from_secs(60)), &token)
    });
    thread::sleep(Duration::from_millis(100));
    cancellation.cancel().unwrap();
    let error = worker.join().unwrap().unwrap_err();
    assert_eq!(error.kind(), &PcscErrorKind::Cancelled);
}

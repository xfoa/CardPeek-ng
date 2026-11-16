//! Opt-in integration tests for real PC/SC readers and test cards.

use std::{ffi::CString, path::PathBuf, thread, time::Duration};

use cardpeek_app::{
    Application, CommandKind, ConnectRequest, EventKind, ExportTrace, OperationId, OperationStatus,
    PcscBackend, TraceExportMode,
};
use cardpeek_core::{ByteString, Protocol, ResetMode, TransportKind};
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

fn wait_terminal(
    events: &cardpeek_app::EventSubscription,
    operation: &OperationId,
) -> OperationStatus {
    loop {
        let event = events
            .recv_timeout(Duration::from_secs(30))
            .expect("application terminal event");
        match event.kind {
            EventKind::OperationCompleted { operation: id } if id == *operation => {
                return OperationStatus::Completed;
            }
            EventKind::OperationCancelled { operation: id } if id == *operation => {
                return OperationStatus::Cancelled;
            }
            EventKind::OperationFailed { operation: id, .. } if id == *operation => {
                return OperationStatus::Failed;
            }
            _ => {}
        }
    }
}

#[test]
#[ignore = "requires a PC/SC reader, a disposable card, and CARDPEEK_PCSC_READER"]
fn application_runs_analyser_and_exports_raw_and_sanitised_traces() {
    let reader = required_env("CARDPEEK_PCSC_READER");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysers");
    let output = std::env::temp_dir().join(format!("cardpeek-hardware-{}", std::process::id()));
    std::fs::create_dir_all(&output).unwrap();
    let app = Application::with_defaults(PcscBackend::establish().unwrap());
    let events = app.subscribe();
    let discovery = app.submit(CommandKind::DiscoverAnalysers(root)).unwrap();
    assert_eq!(
        wait_terminal(&events, &discovery),
        OperationStatus::Completed
    );
    let session = app
        .submit(CommandKind::Connect(ConnectRequest::new(
            reader,
            TransportKind::Pcsc,
        )))
        .unwrap();
    assert_eq!(wait_terminal(&events, &session), OperationStatus::Completed);
    let run = app
        .submit(CommandKind::RunAnalyser(cardpeek_app::RunAnalyserRequest {
            analyser: "atr".into(),
            run_id: cardpeek_core::RunId::new("hardware-atr").unwrap(),
            document_id: cardpeek_core::DocumentId::new("hardware-atr").unwrap(),
        }))
        .unwrap();
    assert_eq!(wait_terminal(&events, &run), OperationStatus::Completed);
    app.export_document_now(&cardpeek_app::ExportDocument {
        run_id: cardpeek_core::RunId::new("hardware-atr").unwrap(),
        destination: output.join("result.json"),
    })
    .unwrap();
    let disconnect = app.submit(CommandKind::Disconnect).unwrap();
    assert_eq!(
        wait_terminal(&events, &disconnect),
        OperationStatus::Completed
    );
    for (mode, name) in [
        (TraceExportMode::Raw, "raw.json"),
        (TraceExportMode::Sanitised, "sanitised.json"),
    ] {
        app.export_trace_now(&ExportTrace {
            session: session.clone(),
            mode,
            destination: output.join(name),
        })
        .unwrap();
    }
    assert_eq!(
        cardpeek_core::TraceDocument::load(output.join("raw.json"))
            .unwrap()
            .sanitised(),
        cardpeek_core::TraceDocument::load(output.join("sanitised.json")).unwrap()
    );
    app.shutdown().unwrap();
    std::fs::remove_dir_all(output).unwrap();
}

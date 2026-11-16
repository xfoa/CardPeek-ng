//! Offline desktop workflow and application-state tests.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cardpeek_app::{
    AnswerInteraction, EventKind, InteractionId, InteractionKind, InteractionRequest,
    InteractionResponse, OperationId, Progress,
};
use cardpeek_core::TraceDocument;
use cardpeek_desktop::state::{DesktopState, ExportKind, RunStatus};
use cardpeek_desktop::{Controller, ReplayBackend};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn controller() -> Controller<ReplayBackend> {
    let trace = TraceDocument::load(fixture("traces/atr-synthetic-001.json")).unwrap();
    Controller::new(
        ReplayBackend::new(trace, "Replay: ATR sample".into()),
        fixture("analysers"),
    )
}

fn wait_for(
    controller: &Controller<ReplayBackend>,
    state: &mut DesktopState,
    condition: impl Fn(&DesktopState) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        controller.poll(state);
        if condition(state) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!(
        "desktop state did not reach the expected condition: {:?}",
        state.errors
    );
}

#[test]
fn replay_atr_workflow_exports_document_and_trace() {
    let controller = controller();
    let mut state = DesktopState::default();
    wait_for(&controller, &mut state, |state| {
        !state.readers.is_empty() && !state.analysers.is_empty()
    });
    assert_eq!(state.selected_analyser.as_deref(), Some("atr"));

    let reader = state.selected_reader.clone().unwrap();
    controller.connect(reader, &mut state);
    wait_for(&controller, &mut state, DesktopState::connected);
    assert!(state.can_run());

    controller.run("atr".into(), &mut state);
    wait_for(&controller, &mut state, |state| {
        state.run_status == RunStatus::Complete && state.document.is_some()
    });
    let document = state.document.as_ref().unwrap();
    assert_eq!(document.document.node_count(), 2);
    assert_eq!(document.revision, 1);
    assert_eq!(state.exchanges.len(), 0);
    assert!(
        state
            .logs
            .iter()
            .any(|entry| entry.message == "Read 2 ATR bytes")
    );

    state.set_search("3B00".into());
    assert_eq!(state.visible_nodes().len(), 1);
    assert_eq!(state.visible_nodes()[0].label, "Answer to Reset");
    assert!(
        state
            .document
            .as_ref()
            .unwrap()
            .document
            .roots
            .iter()
            .all(|root| state.node_visible(root))
    );
    state.set_search("not-present".into());
    assert_eq!(state.visible_nodes().len(), 0);
    assert!(
        state
            .document
            .as_ref()
            .unwrap()
            .document
            .roots
            .iter()
            .all(|root| !state.node_visible(root))
    );
    state.set_search(String::new());

    let directory = std::env::temp_dir().join(format!("cardpeek-desktop-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let document_path = directory.join("result.json");
    state.export_destination = document_path.display().to_string();
    controller.export(&mut state);
    wait_for(&controller, &mut state, |state| {
        state
            .notice
            .as_ref()
            .is_some_and(|notice| notice.starts_with("Exported document"))
    });
    let exported = std::fs::read_to_string(&document_path).unwrap();
    let expected = format!(
        "{}\n",
        state.document.as_ref().unwrap().document.to_json().unwrap()
    );
    assert_eq!(exported, expected);

    controller.disconnect(&mut state);
    wait_for(&controller, &mut state, |state| !state.connected());
    state.export_kind = ExportKind::Trace;
    let trace_path = directory.join("trace.json");
    state.export_destination = trace_path.display().to_string();
    controller.export(&mut state);
    wait_for(&controller, &mut state, |state| {
        state
            .notice
            .as_ref()
            .is_some_and(|notice| notice.starts_with("Exported trace"))
    });
    let exported = TraceDocument::load(&trace_path).unwrap();
    assert_eq!(exported.records.len(), 1);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn interaction_progress_and_cancellation_update_the_state() {
    let mut state = DesktopState::default();
    let operation = OperationId::new("operation-1");
    state.run_operation = Some(operation.clone());
    state.run_status = RunStatus::Running;
    let interaction = InteractionRequest {
        id: InteractionId::new("request-1"),
        kind: InteractionKind::SecretInput {
            prompt: "Enter PIN".into(),
        },
    };
    state.apply_event(event(
        &operation,
        EventKind::InteractionRequested {
            operation: operation.clone(),
            request: interaction.clone(),
        },
    ));
    assert_eq!(state.interaction, Some(interaction));

    let controller = controller();
    controller.answer(
        AnswerInteraction {
            request: InteractionId::new("request-1"),
            response: InteractionResponse::Secret("1234".into()),
        },
        &mut state,
    );
    assert!(state.interaction.is_none());
    state.apply_event(event(
        &operation,
        EventKind::OperationProgress {
            operation: operation.clone(),
            progress: Progress::Determinate {
                completed: 2,
                total: 4,
            },
        },
    ));
    assert_eq!(
        state.progress,
        Some(Progress::Determinate {
            completed: 2,
            total: 4
        })
    );
    state.apply_event(event(
        &operation,
        EventKind::OperationCancelled {
            operation: operation.clone(),
        },
    ));
    assert_eq!(state.run_status, RunStatus::Cancelled);
    assert!(state.progress.is_none());
}

fn event(operation: &OperationId, kind: EventKind) -> cardpeek_app::Event {
    cardpeek_app::Event {
        sequence: 1,
        operation: operation.clone(),
        kind,
        dropped: None,
    }
}

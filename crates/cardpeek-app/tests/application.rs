//! Offline application engine integration tests.

use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use cardpeek_app::{
    AnswerInteraction, AppError, Application, CancellationToken, CommandId, CommandKind,
    ConnectRequest, EngineConfig, EventKind, ExportDocument, ExportTrace, InteractionId,
    InteractionKind, InteractionResponse, LogLevel, OperationId, OperationStatus, SessionBackend,
    transport_error,
};
use cardpeek_core::{
    ApduResponse, Atr, DocumentId, NodeId, ResetMode, ResultDocument, ResultNode, RunId,
    TraceDocument,
};
use cardpeek_replay::ReplaySession;
use cardpeek_transport::{CardSession, SessionCapabilities};

#[derive(Debug)]
struct ReplayAdapter {
    session: ReplaySession,
}

impl CardSession for ReplayAdapter {
    type Error = cardpeek_replay::ReplayError;

    fn protocol(&self) -> cardpeek_core::Protocol {
        cardpeek_core::Protocol::T1
    }

    fn capabilities(&self) -> SessionCapabilities {
        SessionCapabilities {
            extended_apdus: true,
        }
    }

    fn atr(&self) -> Result<Atr, Self::Error> {
        self.session.atr().cloned()
    }

    fn reset(&mut self, mode: ResetMode) -> Result<Atr, Self::Error> {
        match mode {
            ResetMode::Cold => self.session.cold_reset().cloned(),
            ResetMode::Warm => self.session.warm_reset().cloned(),
        }
    }

    fn transmit(&mut self, command: &[u8]) -> Result<ApduResponse, Self::Error> {
        self.session.transmit(command)
    }

    fn disconnect(mut self) -> Result<(), Self::Error> {
        self.session.disconnect();
        Ok(())
    }
}

#[derive(Debug)]
struct TestError;

impl fmt::Display for TestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("cancelled")
    }
}

impl std::error::Error for TestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

#[derive(Debug)]
struct BlockingSession {
    wait: Arc<(Mutex<bool>, Condvar)>,
    started: Arc<(Mutex<bool>, Condvar)>,
    token: CancellationToken,
}

impl CardSession for BlockingSession {
    type Error = TestError;

    fn protocol(&self) -> cardpeek_core::Protocol {
        cardpeek_core::Protocol::T1
    }

    fn capabilities(&self) -> SessionCapabilities {
        SessionCapabilities {
            extended_apdus: true,
        }
    }

    fn atr(&self) -> Result<Atr, Self::Error> {
        Ok(Atr::new(vec![0x3b, 0]))
    }

    fn reset(&mut self, _mode: ResetMode) -> Result<Atr, Self::Error> {
        self.atr()
    }

    fn transmit(&mut self, _command: &[u8]) -> Result<ApduResponse, Self::Error> {
        let (started_lock, started_changed) = &*self.started;
        *started_lock.lock().unwrap() = true;
        started_changed.notify_all();
        let (lock, changed) = &*self.wait;
        let mut cancelled = lock.lock().unwrap();
        while !*cancelled && !self.token.is_cancelled() {
            cancelled = changed.wait(cancelled).unwrap();
        }
        Err(TestError)
    }

    fn disconnect(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct BlockingBackend {
    wait: Arc<(Mutex<bool>, Condvar)>,
    started: Arc<(Mutex<bool>, Condvar)>,
}

impl SessionBackend for BlockingBackend {
    type Session = BlockingSession;

    fn list_readers(&self) -> cardpeek_app::Result<Vec<cardpeek_core::ReaderInfo>> {
        Ok(Vec::new())
    }

    fn connect(&self, request: &ConnectRequest) -> cardpeek_app::Result<Self::Session> {
        Ok(BlockingSession {
            wait: self.wait.clone(),
            started: self.started.clone(),
            token: request.cancellation.clone(),
        })
    }

    fn cancellation_interrupt(
        &self,
        token: &CancellationToken,
    ) -> Option<Box<dyn Fn() + Send + Sync>> {
        let wait = self.wait.clone();
        let token = token.clone();
        Some(Box::new(move || {
            token.cancel();
            let (lock, changed) = &*wait;
            *lock.lock().unwrap() = true;
            changed.notify_all();
        }))
    }
}

struct Backend {
    trace: TraceDocument,
}

impl SessionBackend for Backend {
    type Session = ReplayAdapter;

    fn list_readers(&self) -> cardpeek_app::Result<Vec<cardpeek_core::ReaderInfo>> {
        Ok(vec![cardpeek_core::ReaderInfo {
            name: "Synthetic reader".into(),
            transport: cardpeek_core::TransportKind::Replay,
            card_present: Some(true),
        }])
    }

    fn connect(&self, _request: &ConnectRequest) -> cardpeek_app::Result<Self::Session> {
        let mut session = ReplaySession::new(self.trace.clone());
        session.connect().map_err(transport_error)?;
        Ok(ReplayAdapter { session })
    }
}

fn application(config: EngineConfig) -> Application<Backend> {
    let trace = TraceDocument::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/traces/atr-synthetic-001.json"
    ))
    .unwrap();
    Application::new(Backend { trace }, config)
}

fn connect(app: &Application<Backend>) -> (OperationId, cardpeek_app::EventSubscription) {
    let events = app.subscribe();
    let session = app
        .submit(CommandKind::Connect(ConnectRequest::new(
            "Synthetic reader",
            cardpeek_core::TransportKind::Replay,
        )))
        .unwrap();
    wait_for_terminal(&events, &session);
    assert_eq!(
        app.operation(&session).unwrap().status,
        OperationStatus::Completed
    );
    (session, events)
}

fn wait_for_terminal(events: &cardpeek_app::EventSubscription, operation: &OperationId) {
    for _ in 0..64 {
        let event = events
            .recv_timeout(Duration::from_secs(2))
            .expect("terminal event");
        let terminal = match &event.kind {
            EventKind::OperationCompleted { operation: id }
            | EventKind::OperationFailed { operation: id, .. }
            | EventKind::OperationCancelled { operation: id } => Some(id),
            _ => None,
        };
        if terminal == Some(operation) {
            return;
        }
    }
    panic!("operation did not reach a terminal state");
}

fn next_request(events: &cardpeek_app::EventSubscription) -> InteractionId {
    loop {
        let event = events
            .recv_timeout(Duration::from_secs(2))
            .expect("interaction event");
        if let EventKind::InteractionRequested { request, .. } = event.kind {
            return request.id;
        }
    }
}

fn document(run_id: RunId) -> ResultDocument {
    let mut document = ResultDocument::new(DocumentId::new("app-doc").unwrap(), run_id);
    document.roots.push(ResultNode::new(
        NodeId::new("root").unwrap(),
        "card",
        "Card",
    ));
    document.publish();
    document
}

fn temporary_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "cardpeek-{name}-{}-{:?}.json",
        std::process::id(),
        std::thread::current().id()
    ))
}

#[test]
fn concurrent_frontend_submissions_preserve_session_and_event_order() {
    let app = Arc::new(application(EngineConfig::default()));
    let (session, events) = connect(&app);
    let operations = Arc::new(Mutex::new(Vec::new()));
    let command = vec![0x00, 0xa4, 0x04, 0x00, 0x02, 0xa0, 0x00];
    let mut producers = Vec::new();
    for _ in 0..4 {
        let app = app.clone();
        let operations = operations.clone();
        let command = command.clone();
        producers.push(std::thread::spawn(move || {
            for _ in 0..4 {
                let operation = app.submit(CommandKind::Transmit(command.clone())).unwrap();
                operations.lock().unwrap().push(operation);
            }
        }));
    }
    for producer in producers {
        producer.join().unwrap();
    }
    let submitted = operations.lock().unwrap().clone();
    let mut pending: std::collections::HashSet<_> = submitted.iter().cloned().collect();
    let mut previous_sequence = 0;
    while !pending.is_empty() {
        let event = events
            .recv_timeout(Duration::from_secs(2))
            .expect("concurrent operation event");
        assert!(event.sequence > previous_sequence);
        previous_sequence = event.sequence;
        match event.kind {
            EventKind::OperationCompleted { operation }
            | EventKind::OperationFailed { operation, .. }
            | EventKind::OperationCancelled { operation } => {
                pending.remove(&operation);
            }
            _ => {}
        }
    }
    for operation in submitted {
        assert_eq!(
            app.operation(&operation).unwrap().status,
            OperationStatus::Completed
        );
    }
    assert_eq!(app.session().unwrap().id, session);
    let disconnect = app.submit(CommandKind::Disconnect).unwrap();
    wait_for_terminal(&events, &disconnect);
    assert!(app.session().is_none());
    assert_eq!(app.trace_snapshot(&session).unwrap().records.len(), 17);
    app.shutdown().unwrap();
}

#[test]
fn replay_session_serializes_commands_and_exports_trace() {
    let app = application(EngineConfig::default());
    let (session, events) = connect(&app);
    let command = vec![0x00, 0xa4, 0x04, 0x00, 0x02, 0xa0, 0x00];
    let operations: Vec<_> = (0..8)
        .map(|_| app.submit(CommandKind::Transmit(command.clone())).unwrap())
        .collect();
    for operation in &operations {
        wait_for_terminal(&events, operation);
        assert_eq!(
            app.operation(operation).unwrap().status,
            OperationStatus::Completed
        );
    }
    assert_eq!(app.session().unwrap().id, session);
    assert!(app.trace_snapshot(&session).is_none());
    let disconnect = app.submit(CommandKind::Disconnect).unwrap();
    wait_for_terminal(&events, &disconnect);
    let trace = app.trace_snapshot(&session).unwrap();
    assert_eq!(trace.records.len(), 9);
    let destination = temporary_path("trace");
    app.export_trace_now(&ExportTrace {
        session: session.clone(),
        destination: destination.clone(),
    })
    .unwrap();
    let exported = TraceDocument::load(&destination).unwrap();
    assert_eq!(exported, *trace);
    let _ = std::fs::remove_file(destination);
    app.shutdown().unwrap();
}

#[test]
fn duplicate_command_and_second_session_are_rejected() {
    let app = application(EngineConfig::default());
    let (_session, events) = connect(&app);
    let duplicate_id = CommandId::new("duplicate");
    app.try_submit(cardpeek_app::Command {
        id: duplicate_id.clone(),
        kind: CommandKind::EnumerateReaders,
    })
    .unwrap();
    let duplicate = cardpeek_app::OperationId::new("operation-for-duplicate");
    app.try_submit(cardpeek_app::Command {
        id: duplicate_id,
        kind: CommandKind::EnumerateReaders,
    })
    .unwrap();
    let mut saw_rejection = false;
    for _ in 0..8 {
        let event = events.recv_timeout(Duration::from_secs(2)).unwrap();
        if matches!(
            event.kind,
            EventKind::OperationFailed { ref error, .. }
                if error.contains("duplicate command identifier")
        ) {
            saw_rejection = true;
            break;
        }
    }
    assert!(saw_rejection);
    assert_eq!(
        app.operation(&duplicate).unwrap().status,
        OperationStatus::Completed
    );
    let second = app
        .submit(CommandKind::Connect(ConnectRequest::new(
            "Synthetic reader",
            cardpeek_core::TransportKind::Replay,
        )))
        .unwrap();
    wait_for_terminal(&events, &second);
    let disconnect = app.submit(CommandKind::Disconnect).unwrap();
    wait_for_terminal(&events, &disconnect);
    app.shutdown().unwrap();
}

#[test]
fn interaction_accepts_only_matching_type_and_id() {
    let app = application(EngineConfig::default());
    let (_session, events) = connect(&app);
    let context = app.analyzer_context(
        OperationId::new("interaction-test"),
        RunId::new("interaction-run").unwrap(),
        CancellationToken::new(),
    );
    let worker = std::thread::spawn(move || {
        context.interact(InteractionKind::SecretInput {
            prompt: "PIN".into(),
        })
    });
    let request = next_request(&events);
    assert!(matches!(
        app.answer_interaction(AnswerInteraction {
            request: request.clone(),
            response: InteractionResponse::Text("not secret".into()),
        }),
        Err(AppError::InteractionResponseType)
    ));
    assert!(matches!(
        app.answer_interaction(AnswerInteraction {
            request: InteractionId::new("wrong-request"),
            response: InteractionResponse::Secret("1234".into()),
        }),
        Err(AppError::UnknownInteraction(_))
    ));
    app.answer_interaction(AnswerInteraction {
        request,
        response: InteractionResponse::Secret("1234".into()),
    })
    .unwrap();
    assert_eq!(
        worker.join().unwrap().unwrap(),
        InteractionResponse::Secret("1234".into())
    );
    let disconnect = app.submit(CommandKind::Disconnect).unwrap();
    wait_for_terminal(&events, &disconnect);
    app.shutdown().unwrap();
}

#[test]
fn cancellation_wakes_an_interaction() {
    let app = application(EngineConfig::default());
    let (_session, events) = connect(&app);
    let context = app.analyzer_context(
        OperationId::new("cancellation-test"),
        RunId::new("cancellation-run").unwrap(),
        CancellationToken::new(),
    );
    let token = context.cancellation_token().clone();
    let worker = std::thread::spawn(move || {
        context.interact(InteractionKind::Question {
            prompt: "Continue?".into(),
        })
    });
    let _request = next_request(&events);
    token.cancel();
    assert!(matches!(worker.join().unwrap(), Err(AppError::Cancelled)));
    let disconnect = app.submit(CommandKind::Disconnect).unwrap();
    wait_for_terminal(&events, &disconnect);
    app.shutdown().unwrap();
}

#[test]
fn bounded_event_queue_reports_dropped_logs_and_preserves_terminal_event() {
    let app = application(EngineConfig {
        event_capacity: 2,
        ..EngineConfig::default()
    });
    let events = app.subscribe();
    let run = RunId::new("bounded-run").unwrap();
    app.emit_log(run.clone(), LogLevel::Info, "one");
    app.emit_log(run.clone(), LogLevel::Info, "two");
    app.emit_log(run.clone(), LogLevel::Info, "three");
    app.publish_document(run.clone(), document(run.clone()))
        .unwrap();
    let first = events.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(first.dropped.unwrap().logs, 1);
    let operation = app.submit(CommandKind::EnumerateReaders).unwrap();
    wait_for_terminal(&events, &operation);
    assert_eq!(app.readers().len(), 1);
    app.shutdown().unwrap();
}

#[test]
fn document_snapshots_are_immutable_and_export_to_explicit_path() {
    let app = application(EngineConfig::default());
    let run = RunId::new("document-run").unwrap();
    app.publish_document(run.clone(), document(run.clone()))
        .unwrap();
    let snapshot = app.document_snapshot(&run).unwrap();
    let mut revised = document(run.clone());
    revised.publish();
    app.publish_document(run.clone(), revised).unwrap();
    assert_eq!(snapshot.revision, 1);
    assert_eq!(app.document_snapshot(&run).unwrap().revision, 2);
    let destination = temporary_path("document");
    app.export_document_now(&ExportDocument {
        run_id: run,
        destination: destination.clone(),
    })
    .unwrap();
    assert_eq!(
        ResultDocument::from_json(&std::fs::read_to_string(&destination).unwrap())
            .unwrap()
            .revision,
        2
    );
    let _ = std::fs::remove_file(destination);
    app.shutdown().unwrap();
}

#[test]
fn cancellation_interrupts_native_wait_and_cleans_up() {
    let started = Arc::new((Mutex::new(false), Condvar::new()));
    let backend = BlockingBackend {
        wait: Arc::new((Mutex::new(false), Condvar::new())),
        started: started.clone(),
    };
    let app = Application::with_defaults(backend);
    let events = app.subscribe();
    let session = app
        .submit(CommandKind::Connect(ConnectRequest::new(
            "Blocking reader",
            cardpeek_core::TransportKind::Pcsc,
        )))
        .unwrap();
    wait_for_terminal(&events, &session);
    let transmit = app
        .submit(CommandKind::Transmit(vec![0x00, 0xa4, 0x04, 0x00, 0x00]))
        .unwrap();
    let (started_lock, started_changed) = &*started;
    let mut is_started = started_lock.lock().unwrap();
    while !*is_started {
        is_started = started_changed.wait(is_started).unwrap();
    }
    drop(is_started);
    app.cancel(&transmit).unwrap();
    wait_for_terminal(&events, &transmit);
    assert_eq!(
        app.operation(&transmit).unwrap().status,
        OperationStatus::Cancelled
    );
    let disconnect = app.submit(CommandKind::Disconnect).unwrap();
    wait_for_terminal(&events, &disconnect);
    assert!(app.session().is_none());
    app.shutdown().unwrap();
}

#[test]
fn invalid_apdu_and_unavailable_trace_are_structured_failures() {
    let app = application(EngineConfig::default());
    let (session, events) = connect(&app);
    let operation = app.submit(CommandKind::Transmit(vec![0xff])).unwrap();
    wait_for_terminal(&events, &operation);
    assert_eq!(
        app.operation(&operation).unwrap().status,
        OperationStatus::Failed
    );
    assert!(matches!(
        app.export_trace_now(&ExportTrace {
            session,
            destination: PathBuf::from("unused"),
        }),
        Err(AppError::TraceUnavailable)
    ));
    let disconnect = app.submit(CommandKind::Disconnect).unwrap();
    wait_for_terminal(&events, &disconnect);
    app.shutdown().unwrap();
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use cardpeek_core::{
    ApduCommand, ApduResponse, Atr, ReaderInfo, ResetMode, ResultDocument, TraceDocument,
};
use cardpeek_transport::CardSession;

use crate::state::SharedState;
use crate::sync::{CancellationToken, CommandQueue, EventQueue, lock_unpoisoned};
use crate::{
    AppError, CommandKind, ConnectRequest, Event, EventKind, OperationId, OperationStatus, Result,
    SessionState, emit_to, transport_error,
};

/// Factory for reader discovery and owned card sessions.
pub trait SessionBackend: Send + Sync + 'static {
    /// Blocking card session implementation.
    type Session: CardSession + Send + 'static;

    /// Lists currently available readers.
    ///
    /// # Errors
    ///
    /// Returns a native backend error when discovery fails.
    fn list_readers(&self) -> Result<Vec<ReaderInfo>>;

    /// Connects to one reader.
    ///
    /// # Errors
    ///
    /// Returns a native backend error when connection fails.
    fn connect(&self, request: &ConnectRequest) -> Result<Self::Session>;

    /// Returns a native interrupt callback for one cancellation token.
    ///
    /// Backends that cannot interrupt native waits can return `None`.
    fn cancellation_interrupt(
        &self,
        _token: &CancellationToken,
    ) -> Option<Box<dyn Fn() + Send + Sync>> {
        None
    }
}

/// Blocking operations issued by an analyzer worker.
pub trait AnalyzerHost {
    /// Returns the current Answer to Reset.
    ///
    /// # Errors
    ///
    /// Returns a structured transport error or cancellation error.
    fn atr(&self) -> Result<Atr>;

    /// Resets the active card.
    ///
    /// # Errors
    ///
    /// Returns a structured transport error or cancellation error.
    fn reset(&mut self, mode: ResetMode) -> Result<Atr>;

    /// Transmits exact APDU command bytes.
    ///
    /// # Errors
    ///
    /// Returns a structured transport error or cancellation error.
    fn transmit(&mut self, command: &[u8]) -> Result<ApduResponse>;
}

#[derive(Debug)]
pub(crate) struct WorkerCommand {
    pub(crate) operation: OperationId,
    pub(crate) kind: CommandKind,
    pub(crate) token: CancellationToken,
}

pub(crate) struct SessionRuntime<B: SessionBackend> {
    pub(crate) backend: Arc<B>,
    pub(crate) shared: SharedState,
    pub(crate) subscribers: Arc<std::sync::Mutex<Vec<Arc<EventQueue>>>>,
    pub(crate) session_traces: Arc<std::sync::Mutex<HashMap<OperationId, Arc<TraceDocument>>>>,
    pub(crate) sequence: Arc<std::sync::Mutex<u64>>,
}

impl<B: SessionBackend> SessionRuntime<B> {
    pub(crate) fn spawn(
        self,
        operation: OperationId,
        request: ConnectRequest,
        commands: Arc<CommandQueue<WorkerCommand>>,
    ) -> thread::JoinHandle<()> {
        thread::Builder::new()
            .name(format!("cardpeek-session-{}", operation.as_str()))
            .spawn(move || self.run(&operation, &request, &commands))
            .expect("session worker thread can be created")
    }

    fn run(
        self,
        operation: &OperationId,
        request: &ConnectRequest,
        commands: &CommandQueue<WorkerCommand>,
    ) {
        let reader = ReaderInfo {
            name: request.reader.clone(),
            transport: request.transport,
            card_present: Some(true),
        };
        self.emit(Event::new(
            operation.clone(),
            EventKind::SessionStateChanged {
                session: operation.clone(),
                state: SessionState::Connecting,
                reader: reader.clone(),
            },
        ));
        let token = request.cancellation.clone();
        let session = token.check().and_then(|()| self.backend.connect(request));
        let mut session = match session {
            Ok(session) => session,
            Err(error) => {
                self.fail_pending(operation, &AppError::NoActiveSession);
                self.shared.clear_worker(operation);
                self.fail(operation, &error);
                return;
            }
        };
        let atr = match session.atr().map_err(transport_error) {
            Ok(atr) => atr,
            Err(error) => {
                self.fail_pending(operation, &AppError::NoActiveSession);
                self.shared.clear_worker(operation);
                self.fail(operation, &error);
                return;
            }
        };
        if let Err(error) = token.check() {
            self.shared.clear_worker(operation);
            self.fail(operation, &error);
            return;
        }
        let mut trace = TraceDocument::new();
        trace.record_reset(ResetMode::Cold, atr.clone());
        self.shared.set_session(
            operation,
            reader.clone(),
            session.protocol(),
            session.capabilities(),
            atr,
        );
        self.emit(Event::new(
            operation.clone(),
            EventKind::SessionStateChanged {
                session: operation.clone(),
                state: SessionState::Connected,
                reader: reader.clone(),
            },
        ));
        self.complete(operation);

        let disconnect_operation =
            self.process_commands(operation, &mut session, &mut trace, commands);
        commands.close();
        self.fail_pending(operation, &AppError::Shutdown);
        self.cleanup(operation, reader, session, trace, disconnect_operation);
    }

    fn process_commands<S: CardSession>(
        &self,
        operation: &OperationId,
        session: &mut S,
        trace: &mut TraceDocument,
        commands: &CommandQueue<WorkerCommand>,
    ) -> Option<OperationId> {
        while let Some(command) = commands.pop() {
            let disconnect = matches!(command.kind, CommandKind::Disconnect);
            let disconnect_operation = disconnect.then(|| command.operation.clone());
            self.shared.begin(&command.operation);
            let mut host = SessionHost {
                session_id: operation.clone(),
                session,
                trace,
                token: &command.token,
                operation: &command.operation,
                runtime: self,
            };
            let result = match &command.kind {
                CommandKind::Disconnect => {
                    self.shared.session_state(SessionState::Disconnecting);
                    command.token.check()
                }
                CommandKind::Reset(mode) => host.reset(*mode).map(|_| ()),
                CommandKind::Transmit(bytes) => ApduCommand::parse(bytes)
                    .map_err(AppError::Core)
                    .and_then(|_| host.transmit(bytes).map(|_| ())),
                CommandKind::RunAnalyzer(_) => Err(AppError::AnalyzerRuntimeUnavailable),
                _ => Err(AppError::OperationInProgress),
            };
            match result {
                Ok(()) if !disconnect => self.complete(&command.operation),
                Ok(()) => {}
                Err(error) => self.fail(&command.operation, &error),
            }
            if disconnect {
                return disconnect_operation;
            }
        }
        None
    }

    fn cleanup<S: CardSession>(
        &self,
        operation: &OperationId,
        reader: ReaderInfo,
        session: S,
        trace: TraceDocument,
        disconnect_operation: Option<OperationId>,
    ) {
        let disconnect_result = session.disconnect().map_err(transport_error);
        lock_unpoisoned(&self.session_traces).insert(operation.clone(), Arc::new(trace));
        self.shared.clear_session();
        self.shared.clear_worker(operation);
        self.emit(Event::new(
            operation.clone(),
            EventKind::SessionStateChanged {
                session: operation.clone(),
                state: SessionState::Disconnected,
                reader,
            },
        ));
        if let Some(disconnect_operation) = disconnect_operation {
            match disconnect_result {
                Ok(()) => self.complete(&disconnect_operation),
                Err(error) => self.fail(&disconnect_operation, &error),
            }
        } else if let Err(error) = disconnect_result {
            self.emit(Event::new(
                operation.clone(),
                EventKind::OperationFailed {
                    operation: operation.clone(),
                    error: error.to_string(),
                },
            ));
        }
    }

    fn fail_pending(&self, session: &OperationId, error: &AppError) {
        for pending in self.shared.fail_worker_operations(session) {
            self.emit(Event::new(
                pending.clone(),
                EventKind::OperationFailed {
                    operation: pending,
                    error: error.to_string(),
                },
            ));
        }
    }

    fn complete(&self, operation: &OperationId) {
        if !self.shared.finish(operation, OperationStatus::Completed) {
            return;
        }
        self.emit(Event::new(
            operation.clone(),
            EventKind::OperationCompleted {
                operation: operation.clone(),
            },
        ));
    }

    fn fail(&self, operation: &OperationId, error: &AppError) {
        let cancelled = crate::is_cancellation(error)
            || self
                .shared
                .operation(operation)
                .is_some_and(|operation| operation.status == OperationStatus::Cancelling);
        if !self.shared.finish(
            operation,
            if cancelled {
                OperationStatus::Cancelled
            } else {
                OperationStatus::Failed
            },
        ) {
            return;
        }
        let kind = if cancelled {
            EventKind::OperationCancelled {
                operation: operation.clone(),
            }
        } else {
            EventKind::OperationFailed {
                operation: operation.clone(),
                error: error.to_string(),
            }
        };
        self.emit(Event::new(operation.clone(), kind));
    }

    fn emit(&self, event: Event) {
        emit_to(&self.subscribers, &self.sequence, event);
    }
}

struct SessionHost<'a, S: CardSession, B: SessionBackend> {
    session_id: OperationId,
    session: &'a mut S,
    trace: &'a mut TraceDocument,
    token: &'a CancellationToken,
    operation: &'a OperationId,
    runtime: &'a SessionRuntime<B>,
}

impl<S: CardSession, B: SessionBackend> AnalyzerHost for SessionHost<'_, S, B> {
    fn atr(&self) -> Result<Atr> {
        self.token.check()?;
        self.session.atr().map_err(transport_error)
    }

    fn reset(&mut self, mode: ResetMode) -> Result<Atr> {
        self.token.check()?;
        self.runtime.shared.session_state(SessionState::Resetting);
        let atr = self.session.reset(mode).map_err(transport_error)?;
        self.trace.record_reset(mode, atr.clone());
        self.runtime.shared.update_atr(atr.clone());
        self.runtime.shared.session_state(SessionState::Connected);
        self.token.check()?;
        Ok(atr)
    }

    fn transmit(&mut self, command: &[u8]) -> Result<ApduResponse> {
        self.token.check()?;
        let response = self.session.transmit(command).map_err(transport_error)?;
        let record =
            self.trace
                .record_command(command.to_vec(), response.data.clone(), response.status);
        self.runtime.emit(Event::new(
            self.operation.clone(),
            EventKind::ApduExchanged {
                session: self.session_id.clone(),
                record,
                command: command.to_vec(),
                response: response.clone(),
            },
        ));
        Ok(response)
    }
}

pub(crate) fn export_document(document: &ResultDocument, path: PathBuf) -> Result<()> {
    std::fs::write(path, document.to_json()?).map_err(AppError::Io)
}

pub(crate) fn export_trace(trace: &TraceDocument, path: PathBuf) -> Result<()> {
    trace.save(path).map_err(AppError::Core)
}

#![allow(missing_docs)]

use std::cell::RefCell;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use cardpeek_analyser::AnalyserLibrary;
use cardpeek_app::{
    Application, CommandKind, ConnectRequest, ExportDocument, ExportTrace, RunAnalyserRequest,
    SessionBackend,
};
use cardpeek_core::{ResetMode, TransportKind};

use crate::state::{DesktopState, ExportKind, ExportNotice};

pub enum UiMessage {
    Event(cardpeek_app::Event),
    Error(String),
    Export(Result<ExportNotice, String>),
    ExportDestination(PathBuf),
    InteractionFile(PathBuf),
    AnalyserChanged(Result<Vec<String>, String>),
}

pub struct Controller<B: SessionBackend> {
    application: Arc<Application<B>>,
    analyser_library: AnalyserLibrary,
    messages: Receiver<UiMessage>,
    pending_message: RefCell<Option<UiMessage>>,
    sender: Sender<UiMessage>,
}

impl<B: SessionBackend> PartialEq for Controller<B> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.application, &other.application)
    }
}

impl<B: SessionBackend> Controller<B> {
    pub fn new(backend: B, analyser_library: AnalyserLibrary) -> Self {
        let application = Arc::new(Application::with_defaults(backend));
        let subscription = application.subscribe();
        let (sender, messages) = mpsc::channel();
        let event_sender = sender.clone();
        spawn_task("cardpeek-desktop-events", move || {
            while let Some(event) = subscription.recv() {
                if event_sender.send(UiMessage::Event(event)).is_err() {
                    break;
                }
            }
        });
        let controller = Self {
            application,
            analyser_library,
            messages,
            pending_message: RefCell::new(None),
            sender,
        };
        let _ = controller.submit(CommandKind::EnumerateReaders);
        controller.refresh_analysers();
        controller
    }

    pub fn poll_pending(&self) -> bool {
        if self.pending_message.borrow().is_some() {
            return true;
        }
        match self.messages.try_recv() {
            Ok(message) => {
                self.pending_message.replace(Some(message));
                true
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => false,
        }
    }

    pub fn poll(&self, state: &mut DesktopState) {
        let pending = self.pending_message.take();
        for message in pending.into_iter().chain(self.messages.try_iter()) {
            match message {
                UiMessage::Event(event) => state.apply_event(event),
                UiMessage::Error(error)
                | UiMessage::Export(Err(error))
                | UiMessage::AnalyserChanged(Err(error)) => {
                    state.errors.push(error.clone());
                    state.notice = Some(error);
                }
                UiMessage::Export(Ok(notice)) => state.export_finished(notice),
                UiMessage::ExportDestination(path) => {
                    state.export_destination = path.display().to_string();
                }
                UiMessage::InteractionFile(path) => {
                    state.interaction_file = Some(path);
                }
                UiMessage::AnalyserChanged(Ok(imported)) => {
                    state.notice = Some(if imported.is_empty() {
                        "Analyser library updated.".into()
                    } else {
                        format!("Imported {}.", imported.join(", "))
                    });
                    self.refresh_analysers();
                }
            }
        }
        state.session = self.application.session();
        state.readers = self.application.readers();
        if state.analyser_directory.is_empty() {
            state.analyser_directory = self.analyser_library.root().display().to_string();
        }
        if state.selected_reader.is_none() {
            state.selected_reader = state.readers.first().map(|reader| reader.name.clone());
        }
        let analysers = self.application.analysers();
        if state.analysers != analysers {
            state.analysers = analysers;
        }
        if state.selected_analyser.as_ref().is_none_or(|selected| {
            !state
                .analysers
                .iter()
                .any(|analyser| &analyser.id == selected)
        }) {
            state.selected_analyser = state.analysers.first().map(|analyser| analyser.id.clone());
        }
        if let Some(run_id) = state.run_id.clone()
            && let Some(document) = self.application.document_snapshot(&run_id)
        {
            state.publish_document(document);
        }
    }

    #[must_use]
    pub fn submit(&self, kind: CommandKind) -> Option<cardpeek_app::OperationId> {
        match self.application.submit(kind) {
            Ok(operation) => Some(operation),
            Err(error) => {
                self.send_error(format!("Cannot submit command: {error}"));
                None
            }
        }
    }

    pub fn connect(&self, reader: String, state: &mut DesktopState) {
        state.session = None;
        let transport = state
            .readers
            .iter()
            .find(|candidate| candidate.name == reader)
            .map_or(TransportKind::Replay, |reader| reader.transport);
        let request = ConnectRequest::new(reader, transport);
        if let Some(operation) = self.submit(CommandKind::Connect(request)) {
            state
                .pending
                .insert(operation, cardpeek_app::OperationKind::Connect);
        }
    }

    pub fn disconnect(&self, state: &mut DesktopState) {
        if let Some(operation) = self.submit(CommandKind::Disconnect) {
            state
                .pending
                .insert(operation, cardpeek_app::OperationKind::Disconnect);
        }
    }

    pub fn reset(&self, state: &mut DesktopState) {
        if let Some(operation) = self.submit(CommandKind::Reset(ResetMode::Warm)) {
            state
                .pending
                .insert(operation, cardpeek_app::OperationKind::Reset);
        }
    }

    pub fn transmit(&self, command: &str, state: &mut DesktopState) {
        let parsed = cardpeek_core::ByteString::from_hex(8, command)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                bytes
                    .as_bytes()
                    .map(<[u8]>::to_vec)
                    .ok_or_else(|| "The command must contain complete bytes.".into())
            });
        match parsed {
            Ok(command) => {
                if let Some(operation) = self.submit(CommandKind::Transmit(command)) {
                    state
                        .pending
                        .insert(operation, cardpeek_app::OperationKind::Transmit);
                }
            }
            Err(error) => state.notice = Some(format!("Invalid APDU: {error}")),
        }
    }

    pub fn run(&self, analyser: String, state: &mut DesktopState) {
        let Some((run_id, document_id)) = state.next_run() else {
            return;
        };
        let operation = self
            .application
            .submit(CommandKind::RunAnalyser(RunAnalyserRequest {
                analyser,
                run_id,
                document_id,
            }));
        match operation {
            Ok(operation) => {
                state
                    .pending
                    .insert(operation.clone(), cardpeek_app::OperationKind::RunAnalyser);
                state.run_operation = Some(operation);
            }
            Err(error) => {
                state.run_status = crate::state::RunStatus::Failed;
                self.send_error(format!("Cannot start analyser: {error}"));
            }
        }
    }

    pub fn cancel(&self, state: &DesktopState) {
        if let Some(operation) = &state.run_operation {
            let _ = self.submit(CommandKind::Cancel(operation.clone()));
        }
    }

    pub fn answer(&self, answer: cardpeek_app::AnswerInteraction, state: &mut DesktopState) {
        let _ = self.submit(CommandKind::AnswerInteraction(answer));
        state.interaction = None;
        state.interaction_operation = None;
    }

    pub fn select_export_destination(&self) {
        self.select_file(FileRequest::Export);
    }

    pub fn select_interaction_file(&self, must_exist: bool) {
        self.select_file(FileRequest::Interaction { must_exist });
    }

    pub fn select_analyser_files(&self) {
        let sender = self.sender.clone();
        let library = self.analyser_library.clone();
        spawn_task("cardpeek-desktop-analyser-picker", move || {
            let paths = rfd::FileDialog::new()
                .add_filter("Lua analysers", &["lua"])
                .pick_files();
            if let Some(paths) = paths {
                let result = library
                    .import_files(paths.iter().map(PathBuf::as_path))
                    .map(|imported| imported.into_iter().map(|analyser| analyser.id).collect())
                    .map_err(|error| error.to_string());
                let _ = sender.send(UiMessage::AnalyserChanged(result));
            }
        });
    }

    pub fn remove_analyser(&self, state: &mut DesktopState) {
        let Some(analyser) = state.selected_analyser.clone() else {
            state.notice = Some("Select an analyser to remove.".into());
            return;
        };
        let sender = self.sender.clone();
        let library = self.analyser_library.clone();
        spawn_task("cardpeek-desktop-analyser-removal", move || {
            let result = library
                .remove(&analyser)
                .map(|removed| vec![format!("Removed {}", removed.id)])
                .map_err(|error| error.to_string());
            let _ = sender.send(UiMessage::AnalyserChanged(result));
        });
    }

    fn refresh_analysers(&self) {
        let _ = self.submit(CommandKind::DiscoverAnalysers(
            self.analyser_library.root().to_path_buf(),
        ));
    }

    fn select_file(&self, request: FileRequest) {
        let sender = self.sender.clone();
        spawn_task("cardpeek-desktop-file-dialog", move || {
            let dialog = rfd::FileDialog::new();
            let path = match request {
                FileRequest::Export | FileRequest::Interaction { must_exist: false } => {
                    dialog.save_file()
                }
                FileRequest::Interaction { must_exist: true } => dialog.pick_file(),
            };
            if let Some(path) = path {
                let message = match request {
                    FileRequest::Export => UiMessage::ExportDestination(path),
                    FileRequest::Interaction { .. } => UiMessage::InteractionFile(path),
                };
                let _ = sender.send(message);
            }
        });
    }

    pub fn export(&self, state: &mut DesktopState) {
        let destination = PathBuf::from(state.export_destination.trim());
        if destination.as_os_str().is_empty() {
            state.notice = Some("Choose an export destination.".into());
            return;
        }
        let request = match state.export_kind {
            ExportKind::Document => state.document.as_ref().map(|document| {
                ExportRequest::Document(ExportDocument {
                    run_id: document.run_id.clone(),
                    destination: destination.clone(),
                })
            }),
            ExportKind::Trace => state.last_session.clone().map(|session| {
                ExportRequest::Trace(ExportTrace {
                    session,
                    mode: state.trace_export_mode,
                    destination: destination.clone(),
                })
            }),
        };
        let Some(request) = request else {
            state.notice = Some("The selected export is not available.".into());
            return;
        };
        let application = self.application.clone();
        let sender = self.sender();
        state.show_export = false;
        spawn_task("cardpeek-desktop-export", move || {
            let path = destination.display().to_string();
            let result =
                match request {
                    ExportRequest::Document(export) => application
                        .export_document_now(&export)
                        .map(|()| ExportNotice::Document {
                            run_id: export.run_id,
                            path,
                        }),
                    ExportRequest::Trace(export) => {
                        application
                            .export_trace_now(&export)
                            .map(|()| ExportNotice::Trace {
                                session: export.session,
                                path,
                            })
                    }
                }
                .map_err(|error| error.to_string());
            let _ = sender.send(UiMessage::Export(result));
        });
    }

    fn sender(&self) -> Sender<UiMessage> {
        self.sender.clone()
    }

    fn send_error(&self, message: String) {
        let _ = self.sender.send(UiMessage::Error(message));
    }
}

enum ExportRequest {
    Document(ExportDocument),
    Trace(ExportTrace),
}

enum FileRequest {
    Export,
    Interaction { must_exist: bool },
}

fn spawn_task(name: &str, task: impl FnOnce() + Send + 'static) {
    if let Err(error) = thread::Builder::new().name(name.into()).spawn(task) {
        eprintln!("cardpeek-desktop: cannot create {name}: {error}");
    }
}

#![allow(missing_docs)]

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use cardpeek_analyser::AnalyserMetadata;
use cardpeek_app::{
    DocumentSnapshot, Event, EventKind, InteractionRequest, LogLevel, OperationId, Progress,
    SessionInfo, SessionState,
};
use cardpeek_core::{NodeValue, ResultNode, RunId};

pub const MAX_LOGS: usize = 500;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RunStatus {
    #[default]
    Idle,
    Running,
    Complete,
    Failed,
    Cancelled,
}

impl RunStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "Not run",
            Self::Running => "Running",
            Self::Complete => "Complete",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub sequence: u64,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct Exchange {
    pub record: String,
    pub command: String,
    pub data: String,
    pub status: String,
}

#[derive(Clone, Debug)]
pub enum ExportNotice {
    Document { run_id: RunId, path: String },
    Trace { session: OperationId, path: String },
}

#[derive(Default)]
pub struct DesktopState {
    pub readers: Vec<cardpeek_core::ReaderInfo>,
    pub selected_reader: Option<String>,
    pub analysers: Vec<AnalyserMetadata>,
    pub selected_analyser: Option<String>,
    pub session: Option<SessionInfo>,
    pub document: Option<DocumentSnapshot>,
    pub selected_node: Option<String>,
    pub expanded: HashSet<String>,
    pub search: String,
    pub logs: Vec<LogEntry>,
    pub exchanges: Vec<Exchange>,
    pub interaction: Option<InteractionRequest>,
    pub interaction_operation: Option<OperationId>,
    pub interaction_file: Option<PathBuf>,
    pub run_operation: Option<OperationId>,
    pub run_id: Option<RunId>,
    pub run_status: RunStatus,
    pub progress: Option<Progress>,
    pub pending: HashMap<OperationId, cardpeek_app::OperationKind>,
    pub errors: Vec<String>,
    pub notice: Option<String>,
    pub run_counter: u64,
    pub show_apdus: bool,
    pub show_export: bool,
    pub export_kind: ExportKind,
    pub export_destination: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportKind {
    #[default]
    Document,
    Trace,
}

impl DesktopState {
    #[must_use]
    pub fn connected(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.state == SessionState::Connected)
    }

    #[must_use]
    pub fn run_running(&self) -> bool {
        self.run_status == RunStatus::Running
    }

    #[must_use]
    pub fn can_run(&self) -> bool {
        self.connected()
            && !self.run_running()
            && self.selected_analyser.is_some()
            && !self.pending.values().any(|kind| {
                matches!(
                    kind,
                    cardpeek_app::OperationKind::RunAnalyser
                        | cardpeek_app::OperationKind::Reset
                        | cardpeek_app::OperationKind::Disconnect
                )
            })
    }

    pub fn next_run(&mut self) -> Option<(RunId, cardpeek_core::DocumentId)> {
        self.run_counter += 1;
        let run_id = RunId::new(format!("run-{:04}", self.run_counter)).ok()?;
        let document_id =
            cardpeek_core::DocumentId::new(format!("doc-{:04}", self.run_counter)).ok()?;
        self.run_id = Some(run_id.clone());
        self.run_status = RunStatus::Running;
        self.progress = Some(Progress::Indeterminate);
        self.run_operation = None;
        self.document = None;
        self.selected_node = None;
        self.expanded.clear();
        self.exchanges.clear();
        Some((run_id, document_id))
    }

    pub fn apply_event(&mut self, event: Event) {
        if matches!(
            event.kind,
            EventKind::OperationCompleted { .. }
                | EventKind::OperationFailed { .. }
                | EventKind::OperationCancelled { .. }
        ) {
            self.pending.remove(&event.operation);
        }
        match event.kind {
            EventKind::SessionStateChanged { state, reader, .. } => {
                if state == SessionState::Disconnected {
                    self.session = None;
                } else if let Some(current) = &mut self.session {
                    current.state = state;
                }
                if state == SessionState::Connecting {
                    self.notice = Some(format!("Connecting to {}", reader.name));
                }
                if state == SessionState::Connected {
                    self.notice = Some(format!("Connected to {}", reader.name));
                }
                if state == SessionState::Disconnected {
                    self.notice =
                        Some("Session disconnected. The raw trace can now be exported.".into());
                }
            }
            EventKind::ApduExchanged {
                record,
                command,
                response,
                ..
            } => {
                self.exchanges.push(Exchange {
                    record: record.to_string(),
                    command: hex(&command),
                    data: hex(&response.data),
                    status: format!("{:04X}", response.status.value()),
                });
            }
            EventKind::LogEmitted { level, message, .. } => {
                self.push_log(event.sequence, level, message);
            }
            EventKind::DocumentPublished { run_id, revision } => {
                self.notice = Some(format!(
                    "Document revision {revision} published for {run_id}"
                ));
            }
            EventKind::InteractionRequested { operation, request } => {
                self.interaction_operation = Some(operation);
                self.interaction = Some(request);
            }
            EventKind::OperationProgress {
                operation,
                progress,
            } => {
                if self.run_operation.as_ref() == Some(&operation) {
                    self.progress = Some(progress);
                }
            }
            EventKind::OperationCompleted { operation } => {
                if self.run_operation.as_ref() == Some(&operation) {
                    self.run_status = RunStatus::Complete;
                    self.progress = Some(Progress::Determinate {
                        completed: 1,
                        total: 1,
                    });
                }
            }
            EventKind::OperationFailed { operation, error } => {
                if self.run_operation.as_ref() == Some(&operation) {
                    self.run_status = RunStatus::Failed;
                    self.interaction = None;
                    self.interaction_operation = None;
                    self.interaction_file = None;
                }
                self.errors.push(error.clone());
                self.push_log(event.sequence, LogLevel::Error, error);
            }
            EventKind::OperationCancelled { operation } => {
                if self.run_operation.as_ref() == Some(&operation) {
                    self.run_status = RunStatus::Cancelled;
                    self.progress = None;
                    self.interaction = None;
                    self.interaction_operation = None;
                }
            }
        }
    }

    pub fn publish_document(&mut self, snapshot: DocumentSnapshot) {
        if self.document.as_ref().is_some_and(|current| {
            current.run_id == snapshot.run_id && current.revision >= snapshot.revision
        }) {
            return;
        }
        self.selected_node = first_node(&snapshot.document.roots).map(|node| node.id.to_string());
        self.expanded = snapshot
            .document
            .roots
            .iter()
            .flat_map(collect_branch_ids)
            .collect();
        self.document = Some(snapshot);
    }

    pub fn set_search(&mut self, search: String) {
        self.search = search;
    }

    pub fn visible_nodes(&self) -> Vec<&ResultNode> {
        let Some(document) = &self.document else {
            return Vec::new();
        };
        let query = self.search.trim().to_lowercase();
        document
            .document
            .roots
            .iter()
            .flat_map(flatten)
            .filter(|node| query.is_empty() || node_matches(node, &query))
            .collect()
    }

    #[must_use]
    pub fn node_visible(&self, node: &ResultNode) -> bool {
        let query = self.search.trim().to_lowercase();
        query.is_empty() || subtree_matches(node, &query)
    }

    #[must_use]
    pub fn selected(&self) -> Option<&ResultNode> {
        let document = self.document.as_ref()?;
        let selected = self.selected_node.as_ref()?;
        document
            .document
            .roots
            .iter()
            .find_map(|node| find(node, selected))
    }

    pub fn export_finished(&mut self, notice: ExportNotice) {
        self.show_export = false;
        match notice {
            ExportNotice::Document { run_id, path } => {
                self.notice = Some(format!("Exported document {run_id} to {path}"));
            }
            ExportNotice::Trace { session, path } => {
                self.notice = Some(format!("Exported trace {session} to {path}"));
            }
        }
    }

    fn push_log(&mut self, sequence: u64, level: LogLevel, message: String) {
        if self.logs.len() == MAX_LOGS {
            self.logs.remove(0);
        }
        self.logs.push(LogEntry {
            sequence,
            level,
            message,
        });
    }
}

#[must_use]
pub fn node_value(node: &ResultNode) -> String {
    node.alternative
        .clone()
        .unwrap_or_else(|| match &node.value {
            Some(NodeValue::Text(value)) => value.clone(),
            Some(NodeValue::Bytes(value)) => spaced_hex(value.elements()),
            None => "No value".into(),
        })
}

#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            let _ = write!(output, "{byte:02X}");
            output
        },
    )
}

fn spaced_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn first_node(nodes: &[ResultNode]) -> Option<&ResultNode> {
    nodes.first()
}

fn collect_branch_ids(node: &ResultNode) -> Vec<String> {
    let mut ids = if node.children.is_empty() {
        Vec::new()
    } else {
        vec![node.id.to_string()]
    };
    for child in &node.children {
        ids.extend(collect_branch_ids(child));
    }
    ids
}

fn flatten(node: &ResultNode) -> Vec<&ResultNode> {
    let mut nodes = vec![node];
    for child in &node.children {
        nodes.extend(flatten(child));
    }
    nodes
}

fn find<'a>(node: &'a ResultNode, id: &str) -> Option<&'a ResultNode> {
    if node.id.as_str() == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find(child, id))
}

fn node_matches(node: &ResultNode, query: &str) -> bool {
    let raw_value = match &node.value {
        Some(NodeValue::Text(value)) => value.clone(),
        Some(NodeValue::Bytes(value)) => value.to_hex(),
        None => String::new(),
    };
    format!(
        "{} {} {} {}",
        node.label,
        node.class,
        node_value(node),
        raw_value
    )
    .to_lowercase()
    .contains(query)
}

fn subtree_matches(node: &ResultNode, query: &str) -> bool {
    node_matches(node, query)
        || node
            .children
            .iter()
            .any(|child| subtree_matches(child, query))
}

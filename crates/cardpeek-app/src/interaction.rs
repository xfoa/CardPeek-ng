use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use cardpeek_core::RunId;

use crate::sync::{CancellationToken, EventQueue, InteractionSlot, lock_unpoisoned};
use crate::{
    Event, EventKind, InteractionId, InteractionKind, InteractionRequest, InteractionResponse,
    LogLevel, OperationId, Progress, Result, emit_to,
};

/// Services available to one analyzer operation.
#[derive(Clone)]
pub struct AnalyzerContext {
    operation: OperationId,
    run_id: RunId,
    token: CancellationToken,
    subscribers: Arc<std::sync::Mutex<Vec<Arc<EventQueue>>>>,
    sequence: Arc<std::sync::Mutex<u64>>,
    interactions: Arc<std::sync::Mutex<HashMap<InteractionId, Arc<InteractionSlot>>>>,
    interaction_timeout: Duration,
}

impl AnalyzerContext {
    pub(crate) fn new(
        operation: OperationId,
        run_id: RunId,
        token: CancellationToken,
        subscribers: Arc<std::sync::Mutex<Vec<Arc<EventQueue>>>>,
        sequence: Arc<std::sync::Mutex<u64>>,
        interactions: Arc<std::sync::Mutex<HashMap<InteractionId, Arc<InteractionSlot>>>>,
        interaction_timeout: Duration,
    ) -> Self {
        Self {
            operation,
            run_id,
            token,
            subscribers,
            sequence,
            interactions,
            interaction_timeout,
        }
    }

    /// Returns the cancellation token.
    #[must_use]
    pub const fn cancellation_token(&self) -> &CancellationToken {
        &self.token
    }

    /// Emits determinate or indeterminate progress.
    pub fn progress(&self, progress: Progress) {
        self.emit(EventKind::OperationProgress {
            operation: self.operation.clone(),
            progress,
        });
    }

    /// Emits a structured analyzer log.
    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        self.emit(EventKind::LogEmitted {
            run_id: self.run_id.clone(),
            level,
            message: message.into(),
        });
    }

    /// Requests a correlated response and waits for a response or cancellation.
    ///
    /// # Errors
    ///
    /// Returns cancellation or timeout errors. Response values are never logged.
    pub fn interact(&self, kind: InteractionKind) -> Result<InteractionResponse> {
        self.token.check()?;
        let id = InteractionId::new(format!(
            "interaction-{}-{}",
            self.operation.as_str(),
            next_interaction_id()
        ));
        let request = InteractionRequest {
            id: id.clone(),
            kind,
        };
        let slot = Arc::new(InteractionSlot::new(request.clone()));
        lock_unpoisoned(&self.interactions).insert(id.clone(), slot.clone());
        self.emit(EventKind::InteractionRequested {
            operation: self.operation.clone(),
            request,
        });
        let result = slot.wait(&self.token, self.interaction_timeout);
        lock_unpoisoned(&self.interactions).remove(&id);
        result
    }

    fn emit(&self, kind: EventKind) {
        emit_to(
            &self.subscribers,
            &self.sequence,
            Event::new(self.operation.clone(), kind),
        );
    }
}

fn next_interaction_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

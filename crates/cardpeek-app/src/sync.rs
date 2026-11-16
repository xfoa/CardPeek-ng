use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::{
    AppError, DroppedEvents, Event, EventKind, InteractionRequest, InteractionResponse, Result,
};

/// Cooperative cancellation state for one operation.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    inner: std::sync::Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    state: Mutex<bool>,
    changed: Condvar,
}

impl CancellationToken {
    /// Creates a token in the non-cancelled state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(Inner {
                state: Mutex::new(false),
                changed: Condvar::new(),
            }),
        }
    }

    /// Requests cancellation and wakes waiting operations.
    pub fn cancel(&self) {
        let mut cancelled = lock_unpoisoned(&self.inner.state);
        *cancelled = true;
        self.inner.changed.notify_all();
    }

    /// Reports whether cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        *lock_unpoisoned(&self.inner.state)
    }

    /// Returns an error after cancellation.
    ///
    /// # Errors
    ///
    /// Returns `Cancelled` when cancellation was requested.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(AppError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Creates a handle with no native interrupt callback.
    #[must_use]
    pub fn handle(&self) -> CancellationHandle {
        CancellationHandle::new(self.clone())
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Cancellation state with an optional native interrupt callback.
#[derive(Clone)]
pub struct CancellationHandle {
    token: CancellationToken,
    interrupt: std::sync::Arc<dyn Fn() + Send + Sync>,
}

impl std::fmt::Debug for CancellationHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CancellationHandle")
            .field("cancelled", &self.token.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl CancellationHandle {
    /// Creates a handle with no native interrupt callback.
    #[must_use]
    pub fn new(token: CancellationToken) -> Self {
        Self {
            token,
            interrupt: std::sync::Arc::new(|| {}),
        }
    }

    /// Creates a handle that also interrupts a native operation.
    #[must_use]
    pub fn with_interrupt(
        token: CancellationToken,
        interrupt: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            token,
            interrupt: std::sync::Arc::new(interrupt),
        }
    }

    /// Requests cancellation and interrupts the native operation where supported.
    pub fn cancel(&self) {
        self.token.cancel();
        (self.interrupt)();
    }

    /// Returns the cooperative cancellation token.
    #[must_use]
    pub const fn token(&self) -> &CancellationToken {
        &self.token
    }
}

#[derive(Debug)]
pub(crate) struct EventQueue {
    state: Mutex<EventState>,
    changed: Condvar,
    capacity: usize,
}

#[derive(Debug)]
struct EventState {
    events: VecDeque<Event>,
    closed: bool,
    dropped: DroppedEvents,
}

impl EventQueue {
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "event channel capacity must be positive");
        Self {
            state: Mutex::new(EventState {
                events: VecDeque::new(),
                closed: false,
                dropped: DroppedEvents::default(),
            }),
            changed: Condvar::new(),
            capacity,
        }
    }

    pub(crate) fn push(&self, event: Event) {
        let mut state = lock_unpoisoned(&self.state);
        loop {
            if state.closed {
                return;
            }
            if state.events.len() < self.capacity {
                break;
            }
            match event.kind {
                EventKind::LogEmitted { .. } => {
                    state.dropped.logs += 1;
                    return;
                }
                EventKind::DocumentPublished { ref run_id, .. } => {
                    let index = state.events.iter().position(|queued| {
                        matches!(
                            &queued.kind,
                            EventKind::DocumentPublished { run_id: queued_run, .. }
                                if queued_run == run_id
                        )
                    });
                    if let Some(index) = index {
                        state.events.remove(index);
                    } else {
                        state.dropped.document_notifications += 1;
                        return;
                    }
                    state.dropped.document_notifications += 1;
                }
                EventKind::OperationProgress { .. } => {
                    let index = state.events.iter().position(|queued| {
                        matches!(queued.kind, EventKind::OperationProgress { .. })
                    });
                    if let Some(index) = index {
                        state.events.remove(index);
                        state.dropped.progress_notifications += 1;
                    } else {
                        state.dropped.progress_notifications += 1;
                        return;
                    }
                }
                _ => {
                    while state.events.len() >= self.capacity {
                        if let Some(index) = state.events.iter().position(|queued| {
                            matches!(
                                queued.kind,
                                EventKind::LogEmitted { .. }
                                    | EventKind::DocumentPublished { .. }
                                    | EventKind::OperationProgress { .. }
                            )
                        }) {
                            let removed = state.events.remove(index).expect("index exists");
                            match removed.kind {
                                EventKind::LogEmitted { .. } => state.dropped.logs += 1,
                                EventKind::DocumentPublished { .. } => {
                                    state.dropped.document_notifications += 1;
                                }
                                EventKind::OperationProgress { .. } => {
                                    state.dropped.progress_notifications += 1;
                                }
                                _ => unreachable!(),
                            }
                        } else {
                            state = self
                                .changed
                                .wait(state)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            if state.closed {
                                return;
                            }
                        }
                    }
                }
            }
        }
        if !state.closed {
            state.events.push_back(event);
            self.changed.notify_all();
        }
    }

    pub(crate) fn close(&self) {
        let mut state = lock_unpoisoned(&self.state);
        state.closed = true;
        self.changed.notify_all();
    }

    pub(crate) fn is_closed(&self) -> bool {
        lock_unpoisoned(&self.state).closed
    }

    pub(crate) fn next(&self) -> Option<Event> {
        let mut state = lock_unpoisoned(&self.state);
        loop {
            if let Some(event) = take_next(&mut state) {
                self.changed.notify_all();
                return Some(event);
            }
            if state.closed {
                return None;
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    pub(crate) fn next_timeout(&self, timeout: Duration) -> Option<Event> {
        let deadline = Instant::now() + timeout;
        let mut state = lock_unpoisoned(&self.state);
        loop {
            if let Some(event) = take_next(&mut state) {
                self.changed.notify_all();
                return Some(event);
            }
            if state.closed {
                return None;
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (next, _) = self
                .changed
                .wait_timeout(state, deadline.saturating_duration_since(now))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next;
        }
    }
}

fn take_next(state: &mut EventState) -> Option<Event> {
    let mut event = state.events.pop_front()?;
    if state.dropped != DroppedEvents::default() {
        event.dropped = Some(std::mem::take(&mut state.dropped));
    }
    Some(event)
}

/// Errors from bounded command submission.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SubmitError {
    /// The bounded command queue is full.
    #[error("command queue is full")]
    Full,
    /// The application has shut down.
    #[error("application is shut down")]
    Closed,
}

#[derive(Debug)]
pub(crate) struct CommandQueue<T> {
    state: Mutex<CommandState<T>>,
    changed: Condvar,
    capacity: usize,
}

#[derive(Debug)]
struct CommandState<T> {
    values: VecDeque<T>,
    closed: bool,
}

impl<T> CommandQueue<T> {
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "command channel capacity must be positive");
        Self {
            state: Mutex::new(CommandState {
                values: VecDeque::new(),
                closed: false,
            }),
            changed: Condvar::new(),
            capacity,
        }
    }

    pub(crate) fn push(&self, value: T) -> std::result::Result<(), SubmitError> {
        let mut state = lock_unpoisoned(&self.state);
        if state.closed {
            return Err(SubmitError::Closed);
        }
        if state.values.len() >= self.capacity {
            return Err(SubmitError::Full);
        }
        state.values.push_back(value);
        self.changed.notify_all();
        Ok(())
    }

    pub(crate) fn pop(&self) -> Option<T> {
        let mut state = lock_unpoisoned(&self.state);
        loop {
            if let Some(value) = state.values.pop_front() {
                return Some(value);
            }
            if state.closed {
                return None;
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    pub(crate) fn close(&self) {
        let mut state = lock_unpoisoned(&self.state);
        state.closed = true;
        self.changed.notify_all();
    }
}

#[derive(Debug)]
pub(crate) struct InteractionSlot {
    request: InteractionRequest,
    state: Mutex<InteractionState>,
    changed: Condvar,
}

#[derive(Debug)]
struct InteractionState {
    response: Option<InteractionResponse>,
}

impl InteractionSlot {
    pub(crate) fn new(request: InteractionRequest) -> Self {
        Self {
            request,
            state: Mutex::new(InteractionState { response: None }),
            changed: Condvar::new(),
        }
    }

    pub(crate) fn answer(&self, response: InteractionResponse) -> Result<()> {
        if !response.matches(&self.request.kind) {
            return Err(AppError::InteractionResponseType);
        }
        let mut state = lock_unpoisoned(&self.state);
        if state.response.is_some() {
            return Err(AppError::InteractionAlreadyAnswered);
        }
        state.response = Some(response);
        self.changed.notify_all();
        Ok(())
    }

    pub(crate) fn wait(
        &self,
        cancellation: &CancellationToken,
        timeout: Duration,
    ) -> Result<InteractionResponse> {
        let deadline = Instant::now() + timeout;
        let mut state = lock_unpoisoned(&self.state);
        loop {
            if let Some(response) = state.response.take() {
                return Ok(response);
            }
            cancellation.check()?;
            let now = Instant::now();
            if now >= deadline {
                return Err(AppError::InteractionTimeout);
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(10));
            let (next, _) = self
                .changed
                .wait_timeout(state, wait)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next;
        }
    }
}

pub(crate) fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::{CommandQueue, EventQueue, SubmitError};
    use crate::{Event, EventKind, OperationId};

    #[test]
    fn full_command_queue_rejects_new_work_and_close_rejects_submission() {
        let queue = CommandQueue::new(1);
        queue.push(1).unwrap();
        assert_eq!(queue.push(2), Err(SubmitError::Full));
        assert_eq!(queue.pop(), Some(1));
        queue.close();
        assert_eq!(queue.push(3), Err(SubmitError::Closed));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn closing_a_full_event_queue_unblocks_a_critical_producer() {
        let queue = Arc::new(EventQueue::new(1));
        let event = || {
            Event::new(
                OperationId::new("queue-test"),
                EventKind::OperationCompleted {
                    operation: OperationId::new("queue-test"),
                },
            )
        };
        queue.push(event());
        let producer_queue = queue.clone();
        let producer = thread::spawn(move || producer_queue.push(event()));
        thread::sleep(Duration::from_millis(20));
        assert!(!producer.is_finished());
        queue.close();
        producer.join().unwrap();
        assert!(queue.next_timeout(Duration::ZERO).is_some());
        assert!(queue.next_timeout(Duration::ZERO).is_none());
    }
}

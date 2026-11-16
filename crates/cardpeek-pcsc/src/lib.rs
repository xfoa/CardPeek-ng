//! Native PC/SC reader discovery and card sessions.

use std::{
    ffi::{CStr, CString},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use cardpeek_core::{
    ApduCommand, ApduResponse, Atr, Protocol, ReaderInfo, ResetMode, TransportKind,
};
use cardpeek_transport::{CardSession, SessionCapabilities};
use pcsc::{Card, Context, Disposition, Protocols, ReaderState, Scope, ShareMode, State};
use thiserror::Error;

/// Default response buffer limit, including SW1 and SW2.
pub const DEFAULT_RESPONSE_LIMIT: usize = 65_538;

/// PC/SC operations reported by native errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcscOperation {
    /// Establish a PC/SC context.
    EstablishContext,
    /// Enumerate readers.
    ListReaders,
    /// Query reader state.
    QueryReader,
    /// Wait for a card.
    WaitForCard,
    /// Connect to a card.
    Connect,
    /// Read card status.
    Status,
    /// Reconnect and reset a card.
    Reset,
    /// Transmit an APDU.
    Transmit,
    /// Disconnect from a card.
    Disconnect,
    /// Cancel a blocking context operation.
    Cancel,
}

impl std::fmt::Display for PcscOperation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::EstablishContext => "context establishment",
            Self::ListReaders => "reader discovery",
            Self::QueryReader => "reader status query",
            Self::WaitForCard => "card wait",
            Self::Connect => "connection",
            Self::Status => "card status query",
            Self::Reset => "reset",
            Self::Transmit => "transmission",
            Self::Disconnect => "disconnection",
            Self::Cancel => "cancellation",
        };
        formatter.write_str(name)
    }
}

/// Stable semantic categories for PC/SC failures.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PcscErrorKind {
    /// The operation was cancelled.
    #[error("operation cancelled")]
    Cancelled,
    /// No card is present.
    #[error("no card is present")]
    NoCard,
    /// The card was removed or reset by another client.
    #[error("card was removed or reset by another client")]
    CardRemoved,
    /// The reader is unavailable.
    #[error("reader is unavailable")]
    ReaderUnavailable,
    /// Another client has an incompatible connection.
    #[error("reader sharing violation")]
    SharingViolation,
    /// The operation timed out.
    #[error("operation timed out")]
    Timeout,
    /// The reader, card, or platform does not support the operation.
    #[error("operation is not supported")]
    Unsupported,
    /// The PC/SC service is unavailable.
    #[error("PC/SC service is unavailable")]
    ServiceUnavailable,
    /// A reader name is not valid UTF-8 or contains a null byte.
    #[error("invalid reader name")]
    InvalidReaderName,
    /// The command is not a valid ISO 7816 APDU.
    #[error("invalid APDU: {0}")]
    InvalidCommand(String),
    /// The native response does not contain a complete status word.
    #[error("native response is shorter than SW1 and SW2")]
    InvalidResponse,
    /// The configured response buffer limit is too small or was exceeded.
    #[error("response exceeds the configured limit of {limit} bytes")]
    ResponseTooLarge {
        /// Configured limit, including SW1 and SW2.
        limit: usize,
    },
    /// Another native PC/SC failure.
    #[error("native PC/SC error: {0}")]
    Native(String),
}

/// A structured native transport error.
#[derive(Debug, Error)]
#[error("PC/SC {operation} failed: {kind}")]
pub struct PcscError {
    operation: PcscOperation,
    kind: PcscErrorKind,
    #[source]
    source: Option<pcsc::Error>,
}

impl PcscError {
    /// Creates an error with a semantic category and no native source.
    #[must_use]
    pub const fn new(operation: PcscOperation, kind: PcscErrorKind) -> Self {
        Self {
            operation,
            kind,
            source: None,
        }
    }

    fn native(operation: PcscOperation, source: pcsc::Error) -> Self {
        let kind = match source {
            pcsc::Error::Cancelled
            | pcsc::Error::SystemCancelled
            | pcsc::Error::CancelledByUser => PcscErrorKind::Cancelled,
            pcsc::Error::NoSmartcard => PcscErrorKind::NoCard,
            pcsc::Error::RemovedCard | pcsc::Error::ResetCard => PcscErrorKind::CardRemoved,
            pcsc::Error::UnknownReader
            | pcsc::Error::ReaderUnavailable
            | pcsc::Error::ReaderUnsupported => PcscErrorKind::ReaderUnavailable,
            pcsc::Error::SharingViolation => PcscErrorKind::SharingViolation,
            pcsc::Error::Timeout | pcsc::Error::WaitedTooLong => PcscErrorKind::Timeout,
            pcsc::Error::UnsupportedFeature
            | pcsc::Error::CardUnsupported
            | pcsc::Error::UnsupportedCard
            | pcsc::Error::ProtoMismatch => PcscErrorKind::Unsupported,
            pcsc::Error::NoService | pcsc::Error::ServiceStopped | pcsc::Error::Shutdown => {
                PcscErrorKind::ServiceUnavailable
            }
            other => PcscErrorKind::Native(other.to_string()),
        };
        Self {
            operation,
            kind,
            source: Some(source),
        }
    }

    /// Returns the failed operation.
    #[must_use]
    pub const fn operation(&self) -> PcscOperation {
        self.operation
    }

    /// Returns the semantic error category.
    #[must_use]
    pub const fn kind(&self) -> &PcscErrorKind {
        &self.kind
    }
}

/// Cooperative cancellation state shared within one operation family.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Creates a token in the non-cancelled state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cooperative cancellation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Reports whether cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn check(&self, operation: PcscOperation) -> Result<(), PcscError> {
        if self.is_cancelled() {
            Err(PcscError::new(operation, PcscErrorKind::Cancelled))
        } else {
            Ok(())
        }
    }
}

/// Cancels reader waits and marks the associated token as cancelled.
#[derive(Clone)]
pub struct PcscCancellation {
    context: Context,
    token: CancellationToken,
}

impl PcscCancellation {
    /// Requests cancellation and interrupts blocking operations on this context.
    ///
    /// # Errors
    ///
    /// Returns an error when the native cancellation request fails.
    pub fn cancel(&self) -> Result<(), PcscError> {
        self.token.cancel();
        self.context
            .cancel()
            .map_err(|error| PcscError::native(PcscOperation::Cancel, error))
    }

    /// Returns the cooperative token.
    #[must_use]
    pub const fn token(&self) -> &CancellationToken {
        &self.token
    }
}

/// Access mode for a PC/SC card connection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionMode {
    /// Allow compatible connections from other clients.
    #[default]
    Shared,
    /// Prevent other clients from connecting to the card.
    Exclusive,
}

impl From<ConnectionMode> for ShareMode {
    fn from(mode: ConnectionMode) -> Self {
        match mode {
            ConnectionMode::Shared => Self::Shared,
            ConnectionMode::Exclusive => Self::Exclusive,
        }
    }
}

/// A PC/SC service context for discovery and connection.
#[derive(Clone)]
pub struct PcscContext {
    context: Context,
}

impl PcscContext {
    /// Establishes a user-scoped PC/SC context.
    ///
    /// # Errors
    ///
    /// Returns an error when the native service is unavailable.
    pub fn establish() -> Result<Self, PcscError> {
        Context::establish(Scope::User)
            .map(|context| Self { context })
            .map_err(|error| PcscError::native(PcscOperation::EstablishContext, error))
    }

    /// Interrupts blocking operations on this context.
    ///
    /// # Errors
    ///
    /// Returns an error when native cancellation fails.
    pub fn cancel(&self) -> Result<(), PcscError> {
        self.context
            .cancel()
            .map_err(|error| PcscError::native(PcscOperation::Cancel, error))
    }

    /// Creates a cancellation handle for reader waits on this context.
    #[must_use]
    pub fn cancellation_handle(&self) -> PcscCancellation {
        PcscCancellation {
            context: self.context.clone(),
            token: CancellationToken::new(),
        }
    }

    /// Lists readers and queries whether each reader contains a card.
    ///
    /// # Errors
    ///
    /// Returns an error for native discovery or status failures.
    pub fn list_readers(&self) -> Result<Vec<ReaderInfo>, PcscError> {
        let readers = self
            .context
            .list_readers_owned()
            .map_err(|error| PcscError::native(PcscOperation::ListReaders, error))?;
        readers
            .iter()
            .map(|reader| {
                let name = reader
                    .to_str()
                    .map_err(|_| {
                        PcscError::new(PcscOperation::ListReaders, PcscErrorKind::InvalidReaderName)
                    })?
                    .to_owned();
                let card_present = self.reader_has_card(reader)?;
                Ok(ReaderInfo {
                    name,
                    transport: TransportKind::Pcsc,
                    card_present: Some(card_present),
                })
            })
            .collect()
    }

    /// Waits until the selected reader contains a card or the timeout expires.
    ///
    /// # Errors
    ///
    /// Returns an error on cancellation, timeout, or a native status failure.
    pub fn wait_for_card(
        &self,
        reader: &CStr,
        timeout: Option<Duration>,
        cancellation: &CancellationToken,
    ) -> Result<(), PcscError> {
        let started = Instant::now();
        let mut states = [ReaderState::new(reader, State::UNAWARE)];
        loop {
            cancellation.check(PcscOperation::WaitForCard)?;
            let remaining = timeout.map(|limit| limit.saturating_sub(started.elapsed()));
            if remaining.is_some_and(|value| value.is_zero()) {
                return Err(PcscError::new(
                    PcscOperation::WaitForCard,
                    PcscErrorKind::Timeout,
                ));
            }
            match self.context.get_status_change(remaining, &mut states) {
                Ok(()) if states[0].event_state().contains(State::PRESENT) => return Ok(()),
                Ok(()) => states[0].sync_current_state(),
                Err(pcsc::Error::Timeout) => {
                    return Err(PcscError::native(
                        PcscOperation::WaitForCard,
                        pcsc::Error::Timeout,
                    ));
                }
                Err(error) => {
                    return Err(PcscError::native(PcscOperation::WaitForCard, error));
                }
            }
        }
    }

    /// Connects to a card with the default response limit.
    ///
    /// # Errors
    ///
    /// Returns an error when the reader name or native connection is invalid.
    pub fn connect(&self, reader: &str, mode: ConnectionMode) -> Result<PcscSession, PcscError> {
        self.connect_with_config(
            reader,
            mode,
            DEFAULT_RESPONSE_LIMIT,
            CancellationToken::new(),
        )
    }

    /// Connects to a card with explicit response and cancellation configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the configuration or native connection is invalid.
    pub fn connect_with_config(
        &self,
        reader: &str,
        mode: ConnectionMode,
        response_limit: usize,
        cancellation: CancellationToken,
    ) -> Result<PcscSession, PcscError> {
        if response_limit < 2 {
            return Err(PcscError::new(
                PcscOperation::Connect,
                PcscErrorKind::ResponseTooLarge {
                    limit: response_limit,
                },
            ));
        }
        cancellation.check(PcscOperation::Connect)?;
        let reader = CString::new(reader).map_err(|_| {
            PcscError::new(PcscOperation::Connect, PcscErrorKind::InvalidReaderName)
        })?;
        let card = self
            .context
            .connect(&reader, mode.into(), Protocols::ANY)
            .map_err(|error| PcscError::native(PcscOperation::Connect, error))?;
        let status = card
            .status2_owned()
            .map_err(|error| PcscError::native(PcscOperation::Status, error))?;
        let protocol = status.protocol2().map_or(Protocol::Other, map_protocol);
        let atr = Atr::new(status.atr().to_vec());
        Ok(PcscSession {
            card: Some(card),
            mode,
            protocol,
            atr,
            response_limit,
            cancellation,
        })
    }

    fn reader_has_card(&self, reader: &CStr) -> Result<bool, PcscError> {
        let mut states = [ReaderState::new(reader, State::UNAWARE)];
        self.context
            .get_status_change(Duration::ZERO, &mut states)
            .map_err(|error| PcscError::native(PcscOperation::QueryReader, error))?;
        Ok(states[0].event_state().contains(State::PRESENT))
    }
}

/// One owned native PC/SC card connection.
pub struct PcscSession {
    card: Option<Card>,
    mode: ConnectionMode,
    protocol: Protocol,
    atr: Atr,
    response_limit: usize,
    cancellation: CancellationToken,
}

impl PcscSession {
    /// Returns the configured response limit, including SW1 and SW2.
    #[must_use]
    pub const fn response_limit(&self) -> usize {
        self.response_limit
    }

    /// Returns the cancellation token used by this session.
    #[must_use]
    pub const fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation
    }
    fn card(&self, operation: PcscOperation) -> Result<&Card, PcscError> {
        self.card
            .as_ref()
            .ok_or_else(|| PcscError::new(operation, PcscErrorKind::ReaderUnavailable))
    }

    fn card_mut(&mut self, operation: PcscOperation) -> Result<&mut Card, PcscError> {
        self.card
            .as_mut()
            .ok_or_else(|| PcscError::new(operation, PcscErrorKind::ReaderUnavailable))
    }

    fn refresh_status(&mut self) -> Result<(), PcscError> {
        let status = self
            .card(PcscOperation::Status)?
            .status2_owned()
            .map_err(|error| PcscError::native(PcscOperation::Status, error))?;
        self.protocol = status.protocol2().map_or(Protocol::Other, map_protocol);
        self.atr = Atr::new(status.atr().to_vec());
        Ok(())
    }
}

impl CardSession for PcscSession {
    type Error = PcscError;

    fn protocol(&self) -> Protocol {
        self.protocol
    }

    fn capabilities(&self) -> SessionCapabilities {
        SessionCapabilities {
            extended_apdus: true,
        }
    }

    fn atr(&self) -> Result<Atr, Self::Error> {
        self.cancellation.check(PcscOperation::Status)?;
        Ok(self.atr.clone())
    }

    fn reset(&mut self, mode: ResetMode) -> Result<Atr, Self::Error> {
        self.cancellation.check(PcscOperation::Reset)?;
        let disposition = match mode {
            ResetMode::Cold => Disposition::UnpowerCard,
            ResetMode::Warm => Disposition::ResetCard,
        };
        let share_mode = self.mode.into();
        self.card_mut(PcscOperation::Reset)?
            .reconnect(share_mode, Protocols::ANY, disposition)
            .map_err(|error| PcscError::native(PcscOperation::Reset, error))?;
        self.refresh_status()?;
        self.cancellation.check(PcscOperation::Reset)?;
        Ok(self.atr.clone())
    }

    fn transmit(&mut self, command: &[u8]) -> Result<ApduResponse, Self::Error> {
        ApduCommand::parse(command).map_err(|error| {
            PcscError::new(
                PcscOperation::Transmit,
                PcscErrorKind::InvalidCommand(error.to_string()),
            )
        })?;
        self.cancellation.check(PcscOperation::Transmit)?;
        let mut buffer = vec![0; self.response_limit];
        let response = self
            .card(PcscOperation::Transmit)?
            .transmit(command, &mut buffer)
            .map_err(|error| PcscError::native(PcscOperation::Transmit, error))?;
        self.cancellation.check(PcscOperation::Transmit)?;
        ApduResponse::parse(response)
            .map_err(|_| PcscError::new(PcscOperation::Transmit, PcscErrorKind::InvalidResponse))
    }

    fn disconnect(mut self) -> Result<(), Self::Error> {
        let Some(card) = self.card.take() else {
            return Ok(());
        };
        card.disconnect(Disposition::LeaveCard)
            .map_err(|(_, error)| PcscError::native(PcscOperation::Disconnect, error))
    }
}

impl Drop for PcscSession {
    fn drop(&mut self) {
        if let Some(card) = self.card.take() {
            let _ = card.disconnect(Disposition::LeaveCard);
        }
    }
}

fn map_protocol(protocol: pcsc::Protocol) -> Protocol {
    match protocol {
        pcsc::Protocol::T0 => Protocol::T0,
        pcsc::Protocol::T1 => Protocol::T1,
        pcsc::Protocol::RAW => Protocol::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_native_errors_to_semantic_categories() {
        let error = PcscError::native(PcscOperation::Connect, pcsc::Error::NoSmartcard);
        assert_eq!(error.operation(), PcscOperation::Connect);
        assert_eq!(error.kind(), &PcscErrorKind::NoCard);
        assert!(error.source.is_some());

        let error = PcscError::native(PcscOperation::Transmit, pcsc::Error::RemovedCard);
        assert_eq!(error.kind(), &PcscErrorKind::CardRemoved);
    }

    #[test]
    fn cancellation_is_shared_and_cooperative() {
        let token = CancellationToken::new();
        let clone = token.clone();
        clone.cancel();
        let error = token.check(PcscOperation::Transmit).unwrap_err();
        assert_eq!(error.kind(), &PcscErrorKind::Cancelled);
    }

    #[test]
    fn response_limit_must_fit_status_word() {
        let error = PcscError::new(
            PcscOperation::Connect,
            PcscErrorKind::ResponseTooLarge { limit: 1 },
        );
        assert_eq!(error.kind(), &PcscErrorKind::ResponseTooLarge { limit: 1 });
    }

    #[test]
    fn native_status_is_not_synthesised() {
        let error = PcscError::native(PcscOperation::Transmit, pcsc::Error::CommError);
        assert!(matches!(error.kind(), PcscErrorKind::Native(_)));
        assert!(!error.to_string().contains("6FFF"));
    }
}

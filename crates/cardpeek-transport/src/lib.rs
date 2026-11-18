//! Blocking reader and card transport contracts.

use cardpeek_core::{ApduResponse, Atr, Protocol, ResetMode};

/// Operations supported by an active card session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionCapabilities {
    /// Whether the transport accepts extended APDU encodings.
    pub extended_apdus: bool,
}

/// Current card presence and connection details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardStatus {
    /// Whether a card is present and connected.
    pub card_present: bool,
    /// Negotiated protocol, or `Other` when no card is connected.
    pub protocol: Protocol,
    /// Current ATR, or an empty value when no card is connected.
    pub atr: Atr,
}

/// Blocking operations on one active card connection.
///
/// A session has one owner. Implementations do not retry commands or issue
/// GET RESPONSE commands implicitly.
pub trait CardSession {
    /// Native transport error.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Returns the negotiated protocol.
    fn protocol(&self) -> Protocol;

    /// Returns the transport capabilities.
    fn capabilities(&self) -> SessionCapabilities;

    /// Returns the current Answer to Reset.
    ///
    /// # Errors
    ///
    /// Returns a native error when the ATR is unavailable or cancellation was requested.
    fn atr(&self) -> Result<Atr, Self::Error>;

    /// Returns current card presence and connection details.
    ///
    /// # Errors
    ///
    /// Returns a native error when the reader status cannot be queried.
    fn status(&mut self) -> Result<CardStatus, Self::Error> {
        Ok(CardStatus {
            card_present: true,
            protocol: self.protocol(),
            atr: self.atr()?,
        })
    }

    /// Resets the card and returns its new Answer to Reset.
    ///
    /// # Errors
    ///
    /// Returns a native error when reset, reconnection, or status retrieval fails.
    fn reset(&mut self, mode: ResetMode) -> Result<Atr, Self::Error>;

    /// Transmits exact short or extended command bytes.
    ///
    /// # Errors
    ///
    /// Returns a native error for an invalid command, transport failure, or cancellation.
    fn transmit(&mut self, command: &[u8]) -> Result<ApduResponse, Self::Error>;

    /// Disconnects without resetting or ejecting the card.
    ///
    /// # Errors
    ///
    /// Returns a native error when disconnection fails.
    fn disconnect(self) -> Result<(), Self::Error>;
}

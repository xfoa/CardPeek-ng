use serde::{Deserialize, Serialize};

use crate::{ByteString, CoreError};

/// An Answer to Reset value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Atr(ByteString);

impl Atr {
    /// Creates an ATR from 8-bit bytes.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(ByteString::from_bytes(bytes))
    }

    /// Returns the ATR bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes().unwrap_or_default()
    }

    /// Returns the underlying byte string.
    #[must_use]
    pub const fn as_byte_string(&self) -> &ByteString {
        &self.0
    }

    /// Parses a hexadecimal ATR.
    ///
    /// # Errors
    ///
    /// Returns an error when the input is not a sequence of complete bytes.
    pub fn from_hex(input: &str) -> Result<Self, CoreError> {
        ByteString::from_hex(8, input).map(Self)
    }
}

/// Transmission protocols supported by a card session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    /// ISO 7816 T=0.
    T0,
    /// ISO 7816 T=1.
    T1,
    /// A protocol outside T=0 and T=1.
    Other,
}

/// Identifies a transport implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    /// Native PC/SC.
    Pcsc,
    /// Recorded trace replay.
    Replay,
}

/// Information about a reader or replay source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderInfo {
    /// Reader display name.
    pub name: String,
    /// Transport implementation.
    pub transport: TransportKind,
    /// Whether a card was present during discovery.
    pub card_present: Option<bool>,
}

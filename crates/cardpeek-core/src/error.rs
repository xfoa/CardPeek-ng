use thiserror::Error;

/// Errors produced by core value validation and document formats.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// An element cannot fit in the selected byte-string width.
    #[error("element {value} does not fit in {width}-bit elements")]
    ElementOverflow {
        /// Selected element width.
        width: u8,
        /// Rejected element value.
        value: u8,
    },
    /// A hexadecimal string contains an invalid digit.
    #[error("invalid hexadecimal digit {digit:?} at character {index}")]
    InvalidHexDigit {
        /// Character offset in the input.
        index: usize,
        /// Rejected character.
        digit: char,
    },
    /// A byte string requires an even number of hexadecimal digits.
    #[error("byte input has an odd number of hexadecimal digits")]
    OddHexLength,
    /// An APDU has an invalid structure.
    #[error("invalid APDU: {0}")]
    InvalidApdu(String),
    /// A document or node operation is invalid.
    #[error("invalid document operation: {0}")]
    InvalidDocument(String),
    /// A JSON document cannot be decoded or encoded.
    #[error("JSON document error: {0}")]
    Json(String),
}

impl From<serde_json::Error> for CoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

use cardpeek_core::{ApduResponse, CoreError, StatusWord, TraceRecordId};
use thiserror::Error;

/// Errors produced while accessing a native trace.
#[derive(Debug, Error)]
pub enum TraceError {
    /// Core trace parsing or value validation failed.
    #[error(transparent)]
    Core(#[from] CoreError),
    /// Trace file input or output failed.
    #[error("trace I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Returns a trace record identifier for a one-based record index.
///
/// # Errors
///
/// Returns an error if the generated identifier is empty.
pub fn record_id(index: usize) -> Result<TraceRecordId, TraceError> {
    TraceRecordId::new(format!("record-{index:04}")).map_err(TraceError::Core)
}

/// Builds a response for replay tests without changing replay ordering.
#[must_use]
pub fn response(data: Vec<u8>, status: StatusWord) -> ApduResponse {
    ApduResponse::new(data, status)
}

#[cfg(test)]
mod tests {
    use cardpeek_core::{Atr, ResetMode, StatusWord, TRACE_SCHEMA_VERSION, TraceDocument};

    #[test]
    fn native_json_trace_round_trips() {
        let mut trace = TraceDocument::new();
        trace.record_reset(ResetMode::Cold, Atr::from_hex("3B00").unwrap());
        trace.record_command(
            vec![0, 0xa4, 0, 0],
            vec![0x6f, 0],
            StatusWord::NORMAL_PROCESSING,
        );
        assert_eq!(trace.schema_version, TRACE_SCHEMA_VERSION);
        let encoded = trace.to_json().unwrap();
        assert_eq!(TraceDocument::from_json(&encoded).unwrap(), trace);
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(TraceDocument::from_json("{").is_err());
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let error = TraceDocument::from_json(r#"{"schema_version":2,"records":[]}"#).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported trace schema version 2")
        );
    }

    #[test]
    fn rejects_duplicate_record_identifiers() {
        let mut trace = TraceDocument::new();
        trace.record_reset(ResetMode::Cold, Atr::from_hex("3B00").unwrap());
        trace.records.push(trace.records[0].clone());
        let error = trace.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("duplicate trace record identifier")
        );
    }
}

use std::{collections::HashSet, path::Path};

use serde::{Deserialize, Serialize};

use crate::CoreError;

use crate::{ApduResponse, Atr, StatusWord, TraceRecordId};

/// One reset or command exchange in a trace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TraceRecord {
    /// A cold or warm reset and its ATR.
    Reset {
        /// Stable record identifier.
        id: TraceRecordId,
        /// Reset mode.
        mode: ResetMode,
        /// Answer to Reset.
        atr: Atr,
    },
    /// One command and its card response.
    Command {
        /// Stable record identifier.
        id: TraceRecordId,
        /// Exact command bytes.
        command: Vec<u8>,
        /// Response data and status.
        response: ApduResponse,
    },
}

impl TraceRecord {
    /// Returns the stable record identifier.
    #[must_use]
    pub const fn id(&self) -> &TraceRecordId {
        match self {
            Self::Reset { id, .. } | Self::Command { id, .. } => id,
        }
    }
}

/// Reset behavior represented by a trace record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetMode {
    /// Initial card reset.
    Cold,
    /// Subsequent card reset.
    Warm,
}

/// Current native trace schema version.
pub const TRACE_SCHEMA_VERSION: u32 = 1;

/// An ordered native trace document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceDocument {
    /// Native schema version.
    pub schema_version: u32,
    /// Reset and command records in capture order.
    pub records: Vec<TraceRecord>,
}

impl TraceDocument {
    /// Creates an empty trace.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            schema_version: TRACE_SCHEMA_VERSION,
            records: Vec::new(),
        }
    }

    /// Appends a reset and returns its identifier.
    pub fn record_reset(&mut self, mode: ResetMode, atr: Atr) -> TraceRecordId {
        let id = self.next_id();
        self.records.push(TraceRecord::Reset {
            id: id.clone(),
            mode,
            atr,
        });
        id
    }

    /// Appends a command exchange and returns its identifier.
    pub fn record_command(
        &mut self,
        command: Vec<u8>,
        data: Vec<u8>,
        status: StatusWord,
    ) -> TraceRecordId {
        let id = self.next_id();
        self.records.push(TraceRecord::Command {
            id: id.clone(),
            command,
            response: ApduResponse::new(data, status),
        });
        id
    }

    fn next_id(&self) -> TraceRecordId {
        TraceRecordId::new(format!("record-{:04}", self.records.len() + 1))
            .unwrap_or_else(|_| unreachable!("generated trace identifiers are nonempty"))
    }

    /// Validates the schema version and record identifiers.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported schema or duplicate record identifiers.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.schema_version != TRACE_SCHEMA_VERSION {
            return Err(CoreError::InvalidDocument(format!(
                "unsupported trace schema version {}",
                self.schema_version
            )));
        }
        let mut ids = HashSet::new();
        for record in &self.records {
            if !ids.insert(record.id()) {
                return Err(CoreError::InvalidDocument(format!(
                    "duplicate trace record identifier {}",
                    record.id()
                )));
            }
        }
        Ok(())
    }

    /// Encodes the trace using the versioned native JSON schema.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid trace or a serialization failure.
    pub fn to_json(&self) -> Result<String, CoreError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Decodes and validates a native JSON trace.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed JSON or an invalid trace.
    pub fn from_json(input: &str) -> Result<Self, CoreError> {
        let document: Self = serde_json::from_str(input)?;
        document.validate()?;
        Ok(document)
    }

    /// Loads a trace from an explicit path.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read or parsed.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let input = std::fs::read_to_string(path)
            .map_err(|error| CoreError::InvalidDocument(error.to_string()))?;
        Self::from_json(&input)
    }

    /// Saves a trace to an explicit path.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be written.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        std::fs::write(path, self.to_json()?)
            .map_err(|error| CoreError::InvalidDocument(error.to_string()))
    }
}

impl Default for TraceDocument {
    fn default() -> Self {
        Self::new()
    }
}

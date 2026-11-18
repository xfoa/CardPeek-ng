use std::{collections::HashSet, path::Path};

use serde::{Deserialize, Serialize};

use crate::CoreError;

use crate::{ApduResponse, Atr, ByteString, StatusWord, TraceRecordId};

/// One analyser-defined captured byte string and its source record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturedValue {
    /// Unique name within the capture bundle.
    pub name: String,
    /// Captured native byte string.
    pub bytes: ByteString,
    /// Source trace record.
    pub source: TraceRecordId,
}

/// Versioned analyser output retained with a trace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureBundle {
    /// Stable analyser identifier.
    pub analyser: String,
    /// Analyser version used for collection.
    pub analyser_version: String,
    /// Analyser-defined capture schema.
    pub capture_schema: String,
    /// Zero-based reset segment index.
    pub segment: usize,
    /// Ordered captured values.
    pub values: Vec<CapturedValue>,
}

impl CaptureBundle {
    /// Creates an empty capture bundle.
    #[must_use]
    pub fn new(
        analyser: impl Into<String>,
        analyser_version: impl Into<String>,
        capture_schema: impl Into<String>,
        segment: usize,
    ) -> Self {
        Self {
            analyser: analyser.into(),
            analyser_version: analyser_version.into(),
            capture_schema: capture_schema.into(),
            segment,
            values: Vec::new(),
        }
    }
}

/// One reset-delimited trace segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceSegment<'a> {
    /// Zero-based segment index.
    pub index: usize,
    /// Records before the first reset, or from one reset up to the next reset.
    pub records: &'a [TraceRecord],
}

impl<'a> TraceSegment<'a> {
    /// Returns the reset at the start of this segment, if present.
    #[must_use]
    pub fn reset(&self) -> Option<&'a TraceRecord> {
        match self.records.first() {
            Some(record @ TraceRecord::Reset { .. }) => Some(record),
            _ => None,
        }
    }
}

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

/// Reset behaviour represented by a trace record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetMode {
    /// Initial card reset.
    Cold,
    /// Subsequent card reset.
    Warm,
}

/// Current native trace schema version.
pub const TRACE_SCHEMA_VERSION: u32 = 2;

/// An ordered native trace document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceDocument {
    /// Native schema version.
    pub schema_version: u32,
    /// Reset and command records in capture order.
    pub records: Vec<TraceRecord>,
    /// Analyser capture bundles produced during live collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub captures: Vec<CaptureBundle>,
}

impl TraceDocument {
    /// Creates an empty trace.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            schema_version: TRACE_SCHEMA_VERSION,
            records: Vec::new(),
            captures: Vec::new(),
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

    /// Returns reset-delimited segments, including commands before the first reset.
    #[must_use]
    pub fn segments(&self) -> Vec<TraceSegment<'_>> {
        let mut segments = Vec::new();
        let mut start = 0;
        let mut index = 0;
        for (position, record) in self.records.iter().enumerate() {
            if matches!(record, TraceRecord::Reset { .. }) {
                if position > start || !segments.is_empty() {
                    segments.push(TraceSegment {
                        index,
                        records: &self.records[start..position],
                    });
                    index += 1;
                }
                start = position;
            }
        }
        if start < self.records.len() || segments.is_empty() {
            segments.push(TraceSegment {
                index,
                records: &self.records[start..],
            });
        }
        segments
    }

    /// Returns one reset-delimited segment.
    #[must_use]
    pub fn segment(&self, index: usize) -> Option<TraceSegment<'_>> {
        self.segments()
            .into_iter()
            .find(|segment| segment.index == index)
    }

    /// Extracts one segment while retaining its original record identifiers.
    ///
    /// # Errors
    ///
    /// Returns an error when the segment does not exist.
    pub fn extract_segment(&self, index: usize) -> Result<Self, CoreError> {
        let segment = self.segment(index).ok_or_else(|| {
            CoreError::InvalidDocument(format!("trace segment {index} does not exist"))
        })?;
        let mut trace = Self::new();
        trace.records = segment.records.to_vec();
        trace.captures = self
            .captures
            .iter()
            .filter(|capture| capture.segment == index)
            .cloned()
            .collect();
        Ok(trace)
    }

    /// Returns a copy suitable for sharing with sensitive values redacted.
    ///
    /// Command headers, response status words, record order, and reset modes remain.
    /// Redaction cannot guarantee removal of every card-specific secret.
    #[must_use]
    pub fn sanitised(&self) -> Self {
        let mut sanitised = self.clone();
        for record in &mut sanitised.records {
            match record {
                TraceRecord::Reset { atr, .. } => *atr = Atr::new(Vec::new()),
                TraceRecord::Command {
                    command, response, ..
                } => {
                    if command.len() > 4 {
                        command.truncate(4);
                    }
                    response.data.clear();
                }
            }
        }
        for capture in &mut sanitised.captures {
            for value in &mut capture.values {
                value.bytes = ByteString::new(value.bytes.width());
            }
        }
        sanitised
    }

    fn next_id(&self) -> TraceRecordId {
        TraceRecordId::new(format!("record-{:04}", self.records.len() + 1))
            .unwrap_or_else(|_| unreachable!("generated trace identifiers are nonempty"))
    }

    /// Validates schema versions, record identifiers, and capture provenance.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported schemas or invalid capture bundles.
    pub fn validate(&self) -> Result<(), CoreError> {
        if !matches!(self.schema_version, 1 | TRACE_SCHEMA_VERSION) {
            return Err(CoreError::InvalidDocument(format!(
                "unsupported trace schema version {}",
                self.schema_version
            )));
        }
        if self.schema_version == 1 && !self.captures.is_empty() {
            return Err(CoreError::InvalidDocument(
                "trace schema 1 cannot contain capture bundles".into(),
            ));
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
        for capture in &self.captures {
            for (field, value) in [
                ("analyser", &capture.analyser),
                ("analyser version", &capture.analyser_version),
                ("capture schema", &capture.capture_schema),
            ] {
                if value.trim().is_empty() {
                    return Err(CoreError::InvalidDocument(format!(
                        "capture bundle {field} cannot be empty"
                    )));
                }
            }
            let Some(segment) = self.segment(capture.segment) else {
                return Err(CoreError::InvalidDocument(format!(
                    "capture bundle segment {} does not exist",
                    capture.segment
                )));
            };
            let records: HashSet<_> = segment.records.iter().map(TraceRecord::id).collect();
            let mut names = HashSet::new();
            for value in &capture.values {
                if value.name.trim().is_empty() {
                    return Err(CoreError::InvalidDocument(
                        "captured value name cannot be empty".into(),
                    ));
                }
                if !names.insert(&value.name) {
                    return Err(CoreError::InvalidDocument(format!(
                        "duplicate captured value name {}",
                        value.name
                    )));
                }
                if !records.contains(&value.source) {
                    return Err(CoreError::InvalidDocument(format!(
                        "captured value {} source {} is not in segment {}",
                        value.name, value.source, capture.segment
                    )));
                }
            }
        }
        Ok(())
    }

    /// Encodes the trace using the versioned native JSON schema.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid trace or a serialisation failure.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_one_fixture_round_trips_exactly() {
        let input = r#"{
  "schema_version": 1,
  "records": [
    {
      "kind": "reset",
      "id": "record-0001",
      "mode": "cold",
      "atr": {
        "width": 8,
        "elements": [
          59,
          0
        ]
      }
    }
  ]
}"#;
        let trace = TraceDocument::from_json(input).unwrap();
        assert_eq!(trace.captures, Vec::new());
        assert_eq!(trace.to_json().unwrap(), input);
    }

    #[test]
    fn capture_bundle_round_trips_and_sanitisation_clears_bytes() {
        let mut trace = TraceDocument::new();
        let reset = trace.record_reset(ResetMode::Cold, Atr::from_hex("3B00").unwrap());
        trace.captures.push(CaptureBundle {
            analyser: "atr".into(),
            analyser_version: "1.0.0".into(),
            capture_schema: "atr/v1".into(),
            segment: 0,
            values: vec![CapturedValue {
                name: "atr".into(),
                bytes: ByteString::from_hex(8, "3B00").unwrap(),
                source: reset,
            }],
        });
        let decoded = TraceDocument::from_json(&trace.to_json().unwrap()).unwrap();
        assert_eq!(decoded, trace);
        assert!(!decoded.captures[0].values[0].bytes.is_empty());
        assert!(trace.sanitised().captures[0].values[0].bytes.is_empty());
    }

    #[test]
    fn rejects_invalid_capture_bundles() {
        let mut trace = TraceDocument::new();
        let reset = trace.record_reset(ResetMode::Cold, Atr::from_hex("3B00").unwrap());
        let value = CapturedValue {
            name: "atr".into(),
            bytes: ByteString::from_hex(8, "3B00").unwrap(),
            source: reset,
        };
        trace.captures.push(CaptureBundle {
            analyser: "atr".into(),
            analyser_version: "1".into(),
            capture_schema: "atr/v1".into(),
            segment: 0,
            values: vec![value.clone(), value],
        });
        assert!(
            trace
                .validate()
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        trace.captures[0].values.pop();
        trace.captures[0].segment = 1;
        assert!(
            trace
                .validate()
                .unwrap_err()
                .to_string()
                .contains("does not exist")
        );
        trace.captures[0].segment = 0;
        trace.captures[0].values[0].source = TraceRecordId::new("other").unwrap();
        assert!(
            trace
                .validate()
                .unwrap_err()
                .to_string()
                .contains("not in segment")
        );
    }

    #[test]
    fn segments_include_commands_before_first_reset() {
        let mut trace = TraceDocument::new();
        trace.record_command(vec![0], vec![1], StatusWord::NORMAL_PROCESSING);
        trace.record_reset(ResetMode::Cold, Atr::from_hex("3B00").unwrap());
        trace.record_command(vec![1], vec![2], StatusWord::NORMAL_PROCESSING);
        trace.record_reset(ResetMode::Warm, Atr::from_hex("3B01").unwrap());
        assert_eq!(trace.segments().len(), 3);
        assert!(trace.segment(0).unwrap().reset().is_none());
        assert_eq!(trace.segment(0).unwrap().records.len(), 1);
        assert_eq!(trace.segment(1).unwrap().records.len(), 2);
        assert_eq!(trace.extract_segment(2).unwrap().records.len(), 1);
    }

    #[test]
    fn sanitised_trace_preserves_structure_and_removes_values() {
        let mut trace = TraceDocument::new();
        trace.record_reset(ResetMode::Cold, Atr::new(vec![0x3b, 0x00]));
        trace.record_command(
            vec![0x00, 0xa4, 0x04, 0x00, 0x02, 0xa0, 0x00],
            vec![0x01, 0x02],
            StatusWord::NORMAL_PROCESSING,
        );
        let sanitised = trace.sanitised();
        assert_eq!(sanitised.records.len(), 2);
        match &sanitised.records[0] {
            TraceRecord::Reset { atr, .. } => assert_eq!(atr.as_bytes(), [] as [u8; 0]),
            TraceRecord::Command { .. } => panic!("expected reset"),
        }
        match &sanitised.records[1] {
            TraceRecord::Command {
                command, response, ..
            } => {
                assert_eq!(command, &[0x00, 0xa4, 0x04, 0x00]);
                assert_eq!(response.data, [] as [u8; 0]);
                assert_eq!(response.status, StatusWord::NORMAL_PROCESSING);
            }
            TraceRecord::Reset { .. } => panic!("expected command"),
        }
    }
}

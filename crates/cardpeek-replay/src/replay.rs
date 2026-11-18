use cardpeek_core::{ApduResponse, Atr, TraceDocument, TraceRecord, TraceRecordId};
use thiserror::Error;

/// Replay lifecycle and matching errors.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ReplayError {
    /// An operation needs an active connection.
    #[error("replay session is not connected")]
    NotConnected,
    /// A cold reset requires an initial reset record.
    #[error("trace does not begin with a reset record")]
    MissingColdReset,
    /// No reset has supplied an ATR.
    #[error("no ATR is available")]
    MissingAtr,
    /// A warm reset is outside the selected segment.
    #[error("warm reset is outside replay segment {segment}")]
    ResetOutsideSegment {
        /// Selected zero-based reset segment.
        segment: usize,
    },
    /// No command in the current cycle matches the request.
    #[error("no replay response for command {command} in segment {segment:?}")]
    CommandNotFound {
        /// Uppercase hexadecimal command.
        command: String,
        /// Selected segment for bounded replay.
        segment: Option<usize>,
    },
}

/// One replay response and its source trace record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedResponse {
    /// Recorded card response.
    pub response: ApduResponse,
    /// Actual source trace record identifier.
    pub record: TraceRecordId,
}

/// Stateful replay of one native trace.
#[derive(Clone, Debug)]
pub struct ReplaySession {
    trace: TraceDocument,
    connected: bool,
    current_atr: Option<usize>,
    position: usize,
    segment: Option<usize>,
    start: usize,
    end: usize,
}

impl ReplaySession {
    /// Creates a disconnected replay session.
    #[must_use]
    pub fn new(trace: TraceDocument) -> Self {
        let end = trace.records.len();
        Self {
            trace,
            connected: false,
            current_atr: None,
            position: 0,
            segment: None,
            start: 0,
            end,
        }
    }

    /// Creates a replay session bounded to one reset-delimited segment.
    ///
    /// # Errors
    ///
    /// Returns an error when the segment is absent or has no initial reset.
    pub fn for_segment(trace: TraceDocument, segment: usize) -> Result<Self, ReplayError> {
        let selected = trace.segment(segment).ok_or(ReplayError::CommandNotFound {
            command: String::new(),
            segment: Some(segment),
        })?;
        if selected.reset().is_none() {
            return Err(ReplayError::MissingColdReset);
        }
        let first_id = selected.records[0].id();
        let start = trace
            .records
            .iter()
            .position(|record| record.id() == first_id)
            .unwrap_or(0);
        let end = start + selected.records.len();
        Ok(Self {
            trace,
            connected: false,
            current_atr: None,
            position: start,
            segment: Some(segment),
            start,
            end,
        })
    }

    /// Connects and performs a cold reset.
    ///
    /// # Errors
    ///
    /// Returns an error when the trace has no initial reset record.
    pub fn connect(&mut self) -> Result<&Atr, ReplayError> {
        self.connected = true;
        self.cold_reset()
    }

    /// Disconnects without modifying the trace.
    pub fn disconnect(&mut self) {
        self.connected = false;
    }

    /// Reports whether this session is connected.
    #[must_use]
    pub const fn is_connected(&self) -> bool {
        self.connected
    }

    /// Replays the first reset record.
    ///
    /// # Errors
    ///
    /// Returns an error when disconnected or when the first record is not a reset.
    pub fn cold_reset(&mut self) -> Result<&Atr, ReplayError> {
        if !self.connected {
            return Err(ReplayError::NotConnected);
        }
        if !matches!(
            self.trace.records.get(self.start),
            Some(TraceRecord::Reset { .. })
        ) {
            return Err(ReplayError::MissingColdReset);
        }
        self.select_reset(self.start)
    }

    /// Replays the next reset record, wrapping after the current reset.
    ///
    /// # Errors
    ///
    /// Returns an error when disconnected or when the trace contains no reset.
    pub fn warm_reset(&mut self) -> Result<&Atr, ReplayError> {
        if !self.connected {
            return Err(ReplayError::NotConnected);
        }
        if self.segment.is_some() {
            return Err(ReplayError::ResetOutsideSegment {
                segment: self.segment.unwrap_or(0),
            });
        }
        let start = self.current_atr.unwrap_or(0);
        let index = (start + 1..self.end)
            .chain(self.start..=start)
            .find(|index| matches!(self.trace.records[*index], TraceRecord::Reset { .. }))
            .ok_or(ReplayError::MissingColdReset)?;
        self.select_reset(index)
    }

    /// Returns the most recently selected ATR.
    ///
    /// # Errors
    ///
    /// Returns an error when disconnected or before a reset has supplied an ATR.
    pub fn atr(&self) -> Result<&Atr, ReplayError> {
        if !self.connected {
            return Err(ReplayError::NotConnected);
        }
        let index = self.current_atr.ok_or(ReplayError::MissingAtr)?;
        match &self.trace.records[index] {
            TraceRecord::Reset { atr, .. } => Ok(atr),
            TraceRecord::Command { .. } => Err(ReplayError::MissingAtr),
        }
    }

    /// Finds the next matching command from the current position.
    /// Search wraps to the first command after the active reset.
    ///
    /// # Errors
    ///
    /// Returns an error when disconnected or when no recorded command matches.
    pub fn transmit(&mut self, command: &[u8]) -> Result<ApduResponse, ReplayError> {
        self.transmit_with_record(command)
            .map(|recorded| recorded.response)
    }

    /// Finds a matching command and returns its response and actual record identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when disconnected or no recorded command matches in the segment.
    pub fn transmit_with_record(
        &mut self,
        command: &[u8],
    ) -> Result<RecordedResponse, ReplayError> {
        if !self.connected {
            return Err(ReplayError::NotConnected);
        }
        let start = self.position;
        let after_reset = self.current_atr.map_or(self.start, |index| index + 1);
        let indexes = (start..self.end).chain(after_reset..start);
        for index in indexes {
            if let TraceRecord::Command {
                id,
                command: expected,
                response,
            } = &self.trace.records[index]
                && expected == command
            {
                self.position = next_command_position(&self.trace, index, after_reset, self.end);
                return Ok(RecordedResponse {
                    response: response.clone(),
                    record: id.clone(),
                });
            }
        }
        Err(ReplayError::CommandNotFound {
            command: hex(command),
            segment: self.segment,
        })
    }

    /// Returns the identifier of the active reset record.
    #[must_use]
    pub fn current_reset_id(&self) -> Option<&TraceRecordId> {
        self.current_atr.and_then(|index| self.record_id(index))
    }

    /// Returns the identifier of a record by index.
    #[must_use]
    pub fn record_id(&self, index: usize) -> Option<&TraceRecordId> {
        self.trace.records.get(index).map(TraceRecord::id)
    }

    /// Returns the underlying immutable trace.
    #[must_use]
    pub const fn trace(&self) -> &TraceDocument {
        &self.trace
    }

    fn select_reset(&mut self, index: usize) -> Result<&Atr, ReplayError> {
        let (mode, atr) = match &self.trace.records[index] {
            TraceRecord::Reset { mode, atr, .. } => (*mode, atr),
            TraceRecord::Command { .. } => return Err(ReplayError::MissingColdReset),
        };
        self.current_atr = Some(index);
        self.position = index + 1;
        let _ = mode;
        Ok(atr)
    }
}

fn next_command_position(
    trace: &TraceDocument,
    current: usize,
    after_reset: usize,
    end: usize,
) -> usize {
    let next = current + 1;
    if next < end && matches!(trace.records[next], TraceRecord::Command { .. }) {
        next
    } else {
        after_reset
    }
}

fn hex(bytes: &[u8]) -> String {
    cardpeek_core::ByteString::from_bytes(bytes.to_vec()).to_hex()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardpeek_core::{ResetMode, StatusWord};

    fn trace() -> TraceDocument {
        let mut trace = TraceDocument::new();
        trace.record_reset(ResetMode::Cold, Atr::new(vec![0x3b, 0]));
        trace.record_command(vec![0, 0xa4, 0, 0], vec![1], StatusWord::NORMAL_PROCESSING);
        trace.record_command(vec![0, 0xc0, 0, 0], vec![2], StatusWord::NORMAL_PROCESSING);
        trace.record_reset(ResetMode::Warm, Atr::new(vec![0x3b, 1]));
        trace.record_command(vec![0, 0xa4, 0, 0], vec![3], StatusWord::NORMAL_PROCESSING);
        trace
    }

    #[test]
    fn replays_commands_in_order_and_wraps() {
        let mut session = ReplaySession::new(trace());
        assert_eq!(session.connect().unwrap().as_bytes(), [0x3b, 0]);
        assert_eq!(session.transmit(&[0, 0xa4, 0, 0]).unwrap().data, [1]);
        assert_eq!(session.transmit(&[0, 0xc0, 0, 0]).unwrap().data, [2]);
        assert_eq!(session.transmit(&[0, 0xa4, 0, 0]).unwrap().data, [1]);
    }

    #[test]
    fn warm_reset_selects_next_reset() {
        let mut session = ReplaySession::new(trace());
        session.connect().unwrap();
        assert_eq!(session.warm_reset().unwrap().as_bytes(), [0x3b, 1]);
        assert_eq!(session.transmit(&[0, 0xa4, 0, 0]).unwrap().data, [3]);
    }

    #[test]
    fn bounded_segment_returns_record_ids_and_rejects_other_resets() {
        let mut session = ReplaySession::for_segment(trace(), 1).unwrap();
        assert_eq!(session.connect().unwrap().as_bytes(), [0x3b, 1]);
        let response = session.transmit_with_record(&[0, 0xa4, 0, 0]).unwrap();
        assert_eq!(response.response.data, [3]);
        assert_eq!(response.record.as_str(), "record-0005");
        assert_eq!(
            session.warm_reset().unwrap_err(),
            ReplayError::ResetOutsideSegment { segment: 1 }
        );
        assert_eq!(
            session.transmit(&[0, 0xc0, 0, 0]).unwrap_err(),
            ReplayError::CommandNotFound {
                command: "00C00000".into(),
                segment: Some(1),
            }
        );
    }

    #[test]
    fn mismatch_is_structured_error() {
        let mut session = ReplaySession::new(trace());
        session.connect().unwrap();
        assert_eq!(
            session.transmit(&[0xff]).unwrap_err(),
            ReplayError::CommandNotFound {
                command: "FF".into(),
                segment: None,
            }
        );
    }
}

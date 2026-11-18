//! APDU, card, document, trace, and error types for `Cardpeek` NG.

mod apdu;
mod bytes;
mod card;
mod document;
mod error;
mod ids;
mod status;
mod trace;

pub use apdu::{ApduCase, ApduCommand, ApduResponse};
pub use bytes::ByteString;
pub use card::{Atr, Protocol, ReaderInfo, TransportKind};
pub use document::{DOCUMENT_SCHEMA_VERSION, NodeValue, Provenance, ResultDocument, ResultNode};
pub use error::CoreError;
pub use ids::{DocumentId, NodeId, RunId, TraceRecordId};
pub use status::StatusWord;
pub use trace::{
    CaptureBundle, CapturedValue, ResetMode, TRACE_SCHEMA_VERSION, TraceDocument, TraceRecord,
    TraceSegment,
};

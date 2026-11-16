//! Native trace recording and replay transport.

mod replay;
mod trace;

pub use replay::{ReplayError, ReplaySession};
pub use trace::{TraceError, record_id, response};

#![allow(missing_docs)]

use cardpeek_app::{ConnectRequest, SessionBackend, transport_error};
use cardpeek_core::{
    ApduResponse, Atr, Protocol, ReaderInfo, ResetMode, TraceDocument, TransportKind,
};
use cardpeek_replay::{ReplayError, ReplaySession};
use cardpeek_transport::{CardSession, SessionCapabilities};

pub struct ReplayBackend {
    trace: TraceDocument,
    reader_name: String,
}

impl ReplayBackend {
    #[must_use]
    pub fn new(trace: TraceDocument, reader_name: String) -> Self {
        Self { trace, reader_name }
    }
}

pub struct ReplayAdapter {
    session: ReplaySession,
}

impl CardSession for ReplayAdapter {
    type Error = ReplayError;

    fn protocol(&self) -> Protocol {
        Protocol::T1
    }

    fn capabilities(&self) -> SessionCapabilities {
        SessionCapabilities {
            extended_apdus: true,
        }
    }

    fn atr(&self) -> Result<Atr, Self::Error> {
        self.session.atr().cloned()
    }

    fn reset(&mut self, mode: ResetMode) -> Result<Atr, Self::Error> {
        match mode {
            ResetMode::Cold => self.session.cold_reset().cloned(),
            ResetMode::Warm => self.session.warm_reset().cloned(),
        }
    }

    fn transmit(&mut self, command: &[u8]) -> Result<ApduResponse, Self::Error> {
        self.session.transmit(command)
    }

    fn disconnect(mut self) -> Result<(), Self::Error> {
        self.session.disconnect();
        Ok(())
    }
}

impl SessionBackend for ReplayBackend {
    type Session = ReplayAdapter;

    fn list_readers(&self) -> cardpeek_app::Result<Vec<ReaderInfo>> {
        Ok(vec![ReaderInfo {
            name: self.reader_name.clone(),
            transport: TransportKind::Replay,
            card_present: Some(true),
        }])
    }

    fn connect(&self, _request: &ConnectRequest) -> cardpeek_app::Result<Self::Session> {
        let mut session = ReplaySession::new(self.trace.clone());
        session.connect().map_err(transport_error)?;
        Ok(ReplayAdapter { session })
    }
}

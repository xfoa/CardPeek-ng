#![allow(missing_docs)]

use cardpeek_app::{ConnectRequest, PcscBackend, SessionBackend, transport_error};
use cardpeek_core::{
    ApduResponse, Atr, Protocol, ReaderInfo, ResetMode, TraceDocument, TransportKind,
};
use cardpeek_pcsc::PcscSession;
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

pub struct DesktopBackend {
    replay: ReplayBackend,
    pcsc: Option<PcscBackend>,
}

impl DesktopBackend {
    #[must_use]
    pub fn new(replay: ReplayBackend, pcsc: Option<PcscBackend>) -> Self {
        Self { replay, pcsc }
    }
}

pub enum DesktopSession {
    Replay(ReplayAdapter),
    Pcsc(PcscSession),
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct DesktopSessionError(#[source] Box<dyn std::error::Error + Send + Sync>);

impl DesktopSessionError {
    fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(Box::new(error))
    }
}

impl CardSession for DesktopSession {
    type Error = DesktopSessionError;

    fn protocol(&self) -> Protocol {
        match self {
            Self::Replay(session) => session.protocol(),
            Self::Pcsc(session) => session.protocol(),
        }
    }

    fn capabilities(&self) -> SessionCapabilities {
        match self {
            Self::Replay(session) => session.capabilities(),
            Self::Pcsc(session) => session.capabilities(),
        }
    }

    fn atr(&self) -> Result<Atr, Self::Error> {
        match self {
            Self::Replay(session) => session.atr().map_err(DesktopSessionError::new),
            Self::Pcsc(session) => session.atr().map_err(DesktopSessionError::new),
        }
    }

    fn reset(&mut self, mode: ResetMode) -> Result<Atr, Self::Error> {
        match self {
            Self::Replay(session) => session.reset(mode).map_err(DesktopSessionError::new),
            Self::Pcsc(session) => session.reset(mode).map_err(DesktopSessionError::new),
        }
    }

    fn transmit(&mut self, command: &[u8]) -> Result<ApduResponse, Self::Error> {
        match self {
            Self::Replay(session) => session.transmit(command).map_err(DesktopSessionError::new),
            Self::Pcsc(session) => session.transmit(command).map_err(DesktopSessionError::new),
        }
    }

    fn disconnect(self) -> Result<(), Self::Error> {
        match self {
            Self::Replay(session) => session.disconnect().map_err(DesktopSessionError::new),
            Self::Pcsc(session) => session.disconnect().map_err(DesktopSessionError::new),
        }
    }
}

impl SessionBackend for DesktopBackend {
    type Session = DesktopSession;

    fn list_readers(&self) -> cardpeek_app::Result<Vec<ReaderInfo>> {
        let mut readers = self.replay.list_readers()?;
        if let Some(pcsc) = &self.pcsc {
            readers.extend(pcsc.list_readers()?);
        }
        Ok(readers)
    }

    fn connect(&self, request: &ConnectRequest) -> cardpeek_app::Result<Self::Session> {
        match request.transport {
            TransportKind::Replay => self.replay.connect(request).map(DesktopSession::Replay),
            TransportKind::Pcsc => self
                .pcsc
                .as_ref()
                .ok_or(cardpeek_app::AppError::NoActiveSession)?
                .connect(request)
                .map(DesktopSession::Pcsc),
        }
    }

    fn cancellation_interrupt(
        &self,
        token: &cardpeek_app::CancellationToken,
    ) -> Option<Box<dyn Fn() + Send + Sync>> {
        self.pcsc
            .as_ref()
            .and_then(|backend| backend.cancellation_interrupt(token))
    }
}

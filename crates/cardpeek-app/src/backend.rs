use std::sync::{Arc, Mutex, MutexGuard};

use cardpeek_core::{ReaderInfo, TransportKind};
use cardpeek_pcsc::{ConnectionMode, PcscContext, PcscSession};

use crate::{AppError, CancellationToken, ConnectRequest, Result, SessionBackend, transport_error};

/// Application backend for native PC/SC readers.
#[derive(Clone)]
pub struct PcscBackend {
    context: Arc<Mutex<PcscContext>>,
}

impl PcscBackend {
    /// Establishes a native PC/SC service context.
    ///
    /// # Errors
    ///
    /// Returns an error when the PC/SC service is unavailable.
    pub fn establish() -> Result<Self> {
        PcscContext::establish()
            .map(|context| Self {
                context: Arc::new(Mutex::new(context)),
            })
            .map_err(transport_error)
    }

    fn context(&self) -> Result<MutexGuard<'_, PcscContext>> {
        self.context.lock().map_err(|_| AppError::WorkerPanicked)
    }
}

impl SessionBackend for PcscBackend {
    type Session = PcscSession;

    fn list_readers(&self) -> Result<Vec<ReaderInfo>> {
        self.context()?.list_readers().map_err(transport_error)
    }

    fn connect(&self, request: &ConnectRequest) -> Result<Self::Session> {
        if request.transport != TransportKind::Pcsc {
            return Err(AppError::NoActiveSession);
        }
        let context = self.context()?.clone();
        let reader = std::ffi::CString::new(request.reader.clone()).map_err(|_| {
            transport_error(cardpeek_pcsc::PcscError::new(
                cardpeek_pcsc::PcscOperation::Connect,
                cardpeek_pcsc::PcscErrorKind::InvalidReaderName,
            ))
        })?;
        let cancellation = context.cancellation_handle();
        let token = cancellation.token().clone();
        context
            .wait_for_card(&reader, None, &token)
            .map_err(transport_error)?;
        context
            .connect_with_config(
                &request.reader,
                ConnectionMode::Shared,
                request
                    .response_limit
                    .unwrap_or(cardpeek_pcsc::DEFAULT_RESPONSE_LIMIT),
                token,
            )
            .map_err(transport_error)
    }

    fn cancellation_interrupt(
        &self,
        token: &CancellationToken,
    ) -> Option<Box<dyn Fn() + Send + Sync>> {
        let token = token.clone();
        let context = self.context.clone();
        Some(Box::new(move || {
            token.cancel();
            if let Ok(context) = context.lock() {
                let _ = context.cancel();
            }
        }))
    }
}

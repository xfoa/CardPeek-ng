//! Command-line entry point for `CardPeek` NG.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use cardpeek_app::{
    Application, CommandKind, ConnectRequest, EventKind, OperationId, OperationStatus,
    RunAnalyserRequest, SessionBackend, SubmitError, transport_error,
};
use cardpeek_core::{
    ApduResponse, Atr, Protocol, ReaderInfo, ResetMode, TraceDocument, TransportKind,
};
use cardpeek_replay::{ReplayError, ReplaySession};
use cardpeek_transport::{CardSession, SessionCapabilities};
use clap::{Parser, Subcommand};
use thiserror::Error;

#[derive(Debug, Parser)]
#[command(
    name = "cardpeek",
    version,
    about = "Inspect smart cards with CardPeek-NG analysers"
)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Discover analysers without executing their scripts.
    Discover {
        /// Directory containing Lua analysers.
        analysers: PathBuf,
    },
    /// Run one analyser against a recorded exchange and export its document.
    Replay {
        /// Native JSON trace to replay.
        trace: PathBuf,
        /// Directory containing Lua analysers.
        analysers: PathBuf,
        /// Analyser identifier from static discovery.
        analyser: String,
        /// Explicit native JSON document destination.
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[derive(Debug, Error)]
enum CliError {
    #[error(transparent)]
    Application(#[from] cardpeek_app::AppError),
    #[error(transparent)]
    Submission(#[from] SubmitError),
    #[error(transparent)]
    Core(#[from] cardpeek_core::CoreError),
    #[error(transparent)]
    Replay(#[from] ReplayError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("operation {0} did not reach a terminal state")]
    Timeout(OperationId),
    #[error("operation {operation} failed: {error}")]
    Operation {
        operation: OperationId,
        error: String,
    },
    #[error("analyser {0} was not discovered")]
    UnknownAnalyser(String),
}

type Result<T> = std::result::Result<T, CliError>;

fn main() -> ExitCode {
    match run(Arguments::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cardpeek: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Arguments) -> Result<()> {
    match arguments.command {
        Command::Discover { analysers } => discover(&analysers),
        Command::Replay {
            trace,
            analysers,
            analyser,
            output,
        } => replay(trace, analysers, analyser, output),
    }
}

fn discover(root: &Path) -> Result<()> {
    for analyser in cardpeek_app::discover(root).map_err(cardpeek_app::AppError::from)? {
        println!(
            "{}\t{}\t{}\t{}",
            analyser.id, analyser.version, analyser.title, analyser.description
        );
    }
    Ok(())
}

struct ReplayBackend {
    trace: TraceDocument,
}

struct ReplayAdapter {
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

    fn atr(&self) -> std::result::Result<Atr, Self::Error> {
        self.session.atr().cloned()
    }

    fn reset(&mut self, mode: ResetMode) -> std::result::Result<Atr, Self::Error> {
        match mode {
            ResetMode::Cold => self.session.cold_reset().cloned(),
            ResetMode::Warm => self.session.warm_reset().cloned(),
        }
    }

    fn transmit(&mut self, command: &[u8]) -> std::result::Result<ApduResponse, Self::Error> {
        self.session.transmit(command)
    }

    fn disconnect(mut self) -> std::result::Result<(), Self::Error> {
        self.session.disconnect();
        Ok(())
    }
}

impl SessionBackend for ReplayBackend {
    type Session = ReplayAdapter;

    fn list_readers(&self) -> cardpeek_app::Result<Vec<ReaderInfo>> {
        Ok(vec![ReaderInfo {
            name: "Replay".into(),
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

fn replay(trace: PathBuf, analysers: PathBuf, analyser: String, output: PathBuf) -> Result<()> {
    let fixture_id = trace_file_stem(&trace);
    let trace = TraceDocument::load(trace)?;
    let application = Application::with_defaults(ReplayBackend { trace });
    let events = application.subscribe();
    let discovery = application.submit(CommandKind::DiscoverAnalysers(analysers))?;
    wait(&application, &events, &discovery)?;
    if !application
        .analysers()
        .iter()
        .any(|item| item.id == analyser)
    {
        return Err(CliError::UnknownAnalyser(analyser));
    }
    let session = application.submit(CommandKind::Connect(ConnectRequest::new(
        "Replay",
        TransportKind::Replay,
    )))?;
    wait(&application, &events, &session)?;
    let run_id = cardpeek_core::RunId::new(format!("run-{fixture_id}"))?;
    let run = application.submit(CommandKind::RunAnalyser(RunAnalyserRequest {
        analyser,
        run_id: run_id.clone(),
        document_id: cardpeek_core::DocumentId::new(format!("doc-{fixture_id}"))?,
    }))?;
    wait(&application, &events, &run)?;
    let document = application
        .document_snapshot(&run_id)
        .ok_or_else(|| cardpeek_app::AppError::MissingDocument(run_id.clone()))?;
    application.export_document_now(&cardpeek_app::ExportDocument {
        run_id,
        destination: output,
    })?;
    println!(
        "Exported {} nodes in revision {}",
        document.document.node_count(),
        document.revision
    );
    let disconnect = application.submit(CommandKind::Disconnect)?;
    wait(&application, &events, &disconnect)?;
    application.shutdown()?;
    Ok(())
}

fn trace_file_stem(trace: &std::path::Path) -> String {
    trace
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .filter(|stem| !stem.is_empty())
        .unwrap_or("replay")
        .to_owned()
}

fn wait(
    application: &Application<ReplayBackend>,
    events: &cardpeek_app::EventSubscription,
    operation: &OperationId,
) -> Result<()> {
    for _ in 0..128 {
        let Some(event) = events.recv_timeout(Duration::from_secs(5)) else {
            return Err(CliError::Timeout(operation.clone()));
        };
        match event.kind {
            EventKind::LogEmitted { level, message, .. } => {
                eprintln!("{level:?}: {message}");
            }
            EventKind::OperationCompleted { operation: id } if id == *operation => {
                if application
                    .operation(operation)
                    .is_some_and(|item| item.status == OperationStatus::Completed)
                {
                    return Ok(());
                }
            }
            EventKind::OperationFailed {
                operation: id,
                error,
            } if id == *operation => {
                return Err(CliError::Operation {
                    operation: id,
                    error,
                });
            }
            EventKind::OperationCancelled { operation: id } if id == *operation => {
                return Err(CliError::Operation {
                    operation: id,
                    error: "cancelled".into(),
                });
            }
            _ => {}
        }
    }
    Err(CliError::Timeout(operation.clone()))
}

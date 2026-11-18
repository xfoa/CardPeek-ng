//! Native GPUI desktop entry point.

use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;

use cardpeek_analyser::AnalyserLibrary;
use cardpeek_app::PcscBackend;
use cardpeek_core::TraceDocument;
use cardpeek_desktop::preferences::Preferences;
use cardpeek_desktop::{Controller, DesktopBackend, run};

fn main() -> ExitCode {
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cardpeek-desktop: {error}");
            ExitCode::FAILURE
        }
    }
}

fn launch() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let trace_path = arguments.next().map(PathBuf::from);
    if arguments.next().is_some() {
        return Err("usage: cardpeek-desktop [TRACE_JSON]".into());
    }
    let analyser_library = AnalyserLibrary::user().map_err(|error| error.to_string())?;
    if let Some(path) = &trace_path {
        TraceDocument::load(path).map_err(|error| error.to_string())?;
    }
    let directories = directories::ProjectDirs::from("", "", "cardpeek-ng")
        .ok_or_else(|| "the platform configuration directory is unavailable".to_string())?;
    let preferences = Preferences::load(directories.config_dir().join("desktop-mode"));
    let backend = DesktopBackend::new(None, PcscBackend::establish().ok());
    let controller = Rc::new(Controller::for_desktop(backend, analyser_library));
    run(controller, preferences, trace_path);
    Ok(())
}

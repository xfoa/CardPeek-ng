//! Native Dioxus desktop entry point.

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;

use cardpeek_core::TraceDocument;
use cardpeek_desktop::{App, Controller, ReplayBackend};
use dioxus::native::{Config, LogicalSize, WindowAttributes};
use dioxus::prelude::*;

thread_local! {
    static CONTROLLER: RefCell<Option<Rc<Controller<ReplayBackend>>>> = const { RefCell::new(None) };
}

fn main() -> ExitCode {
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cardpeek-desktop: {error}");
            ExitCode::FAILURE
        }
    }
}

fn app() -> Element {
    CONTROLLER.with(|controller| {
        let controller = controller
            .borrow()
            .clone()
            .expect("desktop controller is installed");
        rsx! { App { controller } }
    })
}

fn launch() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let trace_path = arguments.next().map_or_else(
        || PathBuf::from("tests/fixtures/traces/atr-synthetic-001.json"),
        PathBuf::from,
    );
    let analyser_root = arguments
        .next()
        .map_or_else(|| PathBuf::from("tests/fixtures/analysers"), PathBuf::from);
    if arguments.next().is_some() {
        return Err("usage: cardpeek-desktop [TRACE_JSON] [ANALYSER_DIRECTORY]".into());
    }
    let reader_name = trace_path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .map_or_else(|| "Replay".into(), |stem| format!("Replay: {stem}"));
    let trace = TraceDocument::load(&trace_path).map_err(|error| error.to_string())?;
    let backend = ReplayBackend::new(trace, reader_name);
    let controller = Rc::new(Controller::new(backend, analyser_root));
    CONTROLLER.with(|current| *current.borrow_mut() = Some(controller));
    let config = Config::new().with_window_attributes(
        WindowAttributes::default()
            .with_title("CardPeek-ng")
            .with_inner_size(LogicalSize::new(1280, 820))
            .with_min_inner_size(LogicalSize::new(900, 650)),
    );
    LaunchBuilder::custom(dioxus::native::launch_cfg)
        .with_cfg(config)
        .launch(app);
    Ok(())
}

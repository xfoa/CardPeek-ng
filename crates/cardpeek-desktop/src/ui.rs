#![allow(missing_docs)]

use std::rc::Rc;

use cardpeek_app::{AnswerInteraction, InteractionKind, InteractionResponse, Progress};
use cardpeek_core::ResultNode;
use dioxus::prelude::*;

use crate::ReplayBackend;
use crate::controller::Controller;
use crate::state::{DesktopState, ExportKind, node_value};

const CSS: &str = include_str!("../assets/desktop.css");

pub type DesktopController = Rc<Controller<ReplayBackend>>;

#[component]
pub fn App(controller: DesktopController) -> Element {
    let mut state = use_signal(DesktopState::default);
    let interaction_text = use_signal(String::new);
    let controller_for_poll = controller.clone();
    use_future(move || {
        let controller = controller_for_poll.clone();
        async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if controller.poll_pending() {
                    controller.poll(&mut state.write());
                }
            }
        }
    });

    let snapshot = state.read();
    let connected = snapshot.connected();
    let can_run = snapshot.can_run();
    let running = snapshot.run_running();
    let progress = progress_percent(snapshot.progress.as_ref());
    let reader_name = snapshot
        .selected_reader
        .clone()
        .unwrap_or_else(|| "No replay reader".into());
    let protocol = snapshot
        .session
        .as_ref()
        .map_or("-", |session| match session.protocol {
            cardpeek_core::Protocol::T0 => "T=0",
            cardpeek_core::Protocol::T1 => "T=1",
            cardpeek_core::Protocol::Other => "Other",
        });
    let document_meta = snapshot.document.as_ref().map_or_else(
        || "No result document".into(),
        |document| {
            format!(
                "Document {} / rev. {}",
                document.document.id, document.revision
            )
        },
    );
    let node_count = snapshot
        .document
        .as_ref()
        .map_or(0, |document| document.document.node_count());
    drop(snapshot);

    rsx! {
        style { {CSS} }
        header { class: "app-header",
            div { class: "brand",
                div { class: "brand-mark", aria_hidden: "true" }
                div {
                    div { class: "brand-name", "CardPeek " span { "NG" } }
                    span { class: "prototype-tag", "Replay inspector" }
                }
            }
            div { class: "header-session",
                span { class: "badge badge-replay", "Replay session" }
                span { "{reader_name}" }
                span { class: "badge badge-local", "Local only" }
            }
            div { class: "header-actions",
                button { class: "button button-dark-ghost", onclick: move |_| state.write().show_apdus = true, "Inspect APDUs" }
                button {
                    class: "button button-dark-ghost",
                    onclick: move |_| open_export(&mut state.write(), ExportKind::Document),
                    "Export"
                }
            }
        }
        main { class: "inspector",
            aside { class: "pane setup-pane",
                div { class: "pane-header", h2 { class: "pane-title", "Session setup" } span { class: "badge badge-replay", "Replay" } }
                div { class: "setup-body",
                    section { class: "setup-section",
                        label { class: "section-label", r#for: "reader", "Card reader" }
                        select {
                            class: "select", id: "reader", disabled: connected,
                            onchange: move |event| state.write().selected_reader = Some(event.value()),
                            for reader in state.read().readers.clone() {
                                option { value: "{reader.name}", selected: state.read().selected_reader.as_ref() == Some(&reader.name), "{reader.name}" }
                            }
                        }
                        p { class: "field-note", "Recorded exchanges run without a physical card or reader." }
                        div { class: "status-line small", margin_top: "12px",
                            span { class: if connected { "status-dot" } else { "status-dot offline" } }
                            strong { if connected { "Connected" } else { "Disconnected" } }
                            span { class: "muted", "{protocol}" }
                        }
                        div { class: "setup-actions",
                            if connected {
                                button { class: "button", onclick: { let controller = controller.clone(); move |_| controller.disconnect(&mut state.write()) }, "Disconnect" }
                            } else {
                                button { class: "button", onclick: {
                                    let controller = controller.clone();
                                    move |_| {
                                        let reader = state.read().selected_reader.clone();
                                        if let Some(reader) = reader { controller.connect(reader, &mut state.write()); }
                                    }
                                }, "Connect" }
                            }
                            button { class: "button", disabled: !connected || running, onclick: { let controller = controller.clone(); move |_| controller.reset(&mut state.write()) }, "Reset" }
                        }
                    }
                    section { class: "setup-section",
                        label { class: "section-label", r#for: "analyser", "Analyser" }
                        select {
                            class: "select", id: "analyser", disabled: running,
                            onchange: move |event| state.write().selected_analyser = Some(event.value()),
                            for analyser in state.read().analysers.clone() {
                                option { value: "{analyser.id}", selected: state.read().selected_analyser.as_ref() == Some(&analyser.id), "{analyser.title}" }
                            }
                        }
                        if let Some(analyser) = state.read().analysers.iter().find(|analyser| Some(&analyser.id) == state.read().selected_analyser.as_ref()).cloned() {
                            p { class: "field-note", "{analyser.title} {analyser.version}" br {} "{analyser.description}" }
                        }
                        button {
                            class: "button button-primary button-wide", disabled: !can_run,
                            onclick: {
                                let controller = controller.clone();
                                move |_| {
                                    let analyser = { state.read().selected_analyser.clone() };
                                    if let Some(analyser) = analyser {
                                        controller.run(analyser, &mut state.write());
                                    }
                                }
                            },
                            if running { "Running analyser" } else { "Run analyser" }
                        }
                        div { margin_top: "14px",
                            div { class: "progress-track", div { class: "progress-bar", width: "{progress}%" } }
                            div { class: "progress-meta", span { "{state.read().run_status.label()}" } span { "{progress}%" } }
                        }
                        if running {
                            button { class: "button button-wide", margin_top: "8px", onclick: { let controller = controller.clone(); move |_| controller.cancel(&state.read()) }, "Cancel run" }
                        }
                    }
                    section { class: "setup-section",
                        div { class: "notice", strong { "Trusted scripts only" } "Analysers can access local files and system facilities. They are not sandboxed." }
                        div { class: "notice notice-safe", margin_top: "12px", strong { "No telemetry" } "Card data and traces stay on this device unless you export them." }
                    }
                }
            }
            div { class: "inspector-workspace",
                div { class: "workspace-heading",
                    div { h1 { "Analysis result" } p { "{document_meta}" } }
                    div { class: "search-wrap",
                        input {
                            class: "text-input", r#type: "search", placeholder: "Search labels and values",
                            aria_label: "Search result tree", value: "{state.read().search}",
                            oninput: move |event| state.write().set_search(event.value()),
                        }
                    }
                }
                div { class: "inspector-results",
                    section { class: "pane results-tree-pane", aria_label: "Result tree",
                        div { class: "pane-header", h2 { class: "pane-title", "Result tree" } span { class: "muted small", "{node_count} nodes" } }
                        ResultTree { state }
                    }
                    section { class: "pane details-pane", aria_label: "Selected node details",
                        NodeDetails { state }
                    }
                }
                section { class: "pane activity-pane", aria_label: "Analyser activity",
                    div { class: "pane-header", h2 { class: "pane-title", "Analyser activity" }
                        button { class: "button button-quiet", onclick: move |_| state.write().logs.clear(), "Clear log" }
                    }
                    div { class: "activity-body",
                        ul { class: "log-list",
                            for log in state.read().logs.iter().rev().take(12).cloned().collect::<Vec<_>>() {
                                li { key: "{log.sequence}",
                                    span { class: "log-time", "#{log.sequence}" }
                                    span { class: "log-level", "{log.level:?}" }
                                    span { "{log.message}" }
                                }
                            }
                        }
                        div { class: "activity-summary",
                            div { class: "metric-line", strong { "{state.read().exchanges.len()}" } span { "APDU exchanges" } }
                            div { class: "progress-meta", span { "{node_count} result nodes" } }
                            button { class: "button", onclick: move |_| state.write().show_apdus = true, "View exchanges" }
                        }
                    }
                }
            }
        }
        if state.read().interaction.is_some() {
            InteractionDialog { state, interaction_text, controller: controller.clone() }
        }
        if state.read().show_apdus {
            ApduDialog { state }
        }
        if state.read().show_export {
            ExportDialog { state, controller: controller.clone() }
        }
        if let Some(notice) = state.read().notice.clone() {
            div { class: "toast", role: "status", "{notice}" }
        }
    }
}

#[component]
fn ResultTree(state: Signal<DesktopState>) -> Element {
    let snapshot = state.read();
    let Some(document) = &snapshot.document else {
        return rsx! { div { class: "empty-state", "Connect and run an analyser to publish a result document." } };
    };
    let roots = document.document.roots.clone();
    let searching = !snapshot.search.trim().is_empty();
    let visible = snapshot.visible_nodes().len();
    drop(snapshot);
    rsx! {
        div { class: "result-tree",
            if searching && visible == 0 {
                div { class: "empty-state", "No matching result nodes." }
            } else {
                ul { class: "tree-group",
                    for node in roots {
                        TreeNode { key: "{node.id}", node, state, depth: 0, searching }
                    }
                }
            }
        }
    }
}

#[component]
fn TreeNode(
    node: ResultNode,
    state: Signal<DesktopState>,
    depth: usize,
    searching: bool,
) -> Element {
    if !state.read().node_visible(&node) {
        return rsx! {};
    }
    let id = node.id.to_string();
    let expanded = state.read().expanded.contains(&id);
    let selected = state.read().selected_node.as_ref() == Some(&id);
    let value = node_value(&node);
    let has_children = !node.children.is_empty();
    let children = node.children.clone();
    rsx! {
        li {
            button {
                class: if selected { "tree-node selected" } else { "tree-node" },
                onclick: move |_| {
                    let mut state = state.write();
                    state.selected_node = Some(id.clone());
                    if has_children && !searching && !state.expanded.remove(&id) {
                        state.expanded.insert(id.clone());
                    }
                },
                span { class: "tree-chevron", if has_children { if expanded || searching { "v" } else { ">" } } }
                span { class: "node-icon", "{node.class.chars().next().unwrap_or('#')}" }
                "{node.label}"
                span { class: "tree-value", "{value}" }
            }
            if has_children && (expanded || searching) {
                ul { class: "tree-group",
                    for child in children {
                        TreeNode { key: "{child.id}", node: child, state, depth: depth + 1, searching }
                    }
                }
            }
        }
    }
}

#[component]
fn NodeDetails(state: Signal<DesktopState>) -> Element {
    let snapshot = state.read();
    let Some(node) = snapshot.selected().cloned() else {
        return rsx! { div { class: "empty-state", "Select a result node." } };
    };
    let size = node
        .size_bits
        .map_or_else(|| "Not specified".into(), |size| format!("{size} bits"));
    let mime_type = node.mime_type.as_deref().unwrap_or("Not specified");
    let source = node.provenance.as_ref().map_or_else(
        || "Not specified".into(),
        |source| source.record.to_string(),
    );
    rsx! {
        div { class: "pane-header", h2 { class: "pane-title", "Node details" } span { class: "muted small mono", "{node.id}" } }
        div { class: "details-body",
            h2 { "{node.label}" }
            p { "Selected result node" }
            dl { class: "detail-grid",
                dt { "Value" } dd { code { class: "code-value", "{node_value(&node)}" } }
                dt { "Class" } dd { class: "mono", "{node.class}" }
                dt { "Size" } dd { "{size}" }
                dt { "Node ID" } dd { class: "mono", "{node.id}" }
                dt { "MIME type" } dd { class: "mono", "{mime_type}" }
                dt { "Source" } dd { "{source}" }
            }
        }
    }
}

#[component]
fn ApduDialog(state: Signal<DesktopState>) -> Element {
    rsx! {
        DialogFrame { title: "APDU exchanges", subtitle: "Exact replay command and response records", wide: true, onclose: move |()| state.write().show_apdus = false,
            div { class: "dialog-body",
                if state.read().exchanges.is_empty() {
                    div { class: "empty-state", "No APDU commands were exchanged in this run." }
                }
                for exchange in state.read().exchanges.clone() {
                    div { class: "apdu-row",
                        span { class: "apdu-direction", "TX" } span { class: "muted small", "{exchange.record}" }
                        code { "{exchange.command}" }
                    }
                    div { class: "apdu-row",
                        span { class: "apdu-direction", "RX" } span { class: "muted small", "Status {exchange.status}" }
                        code { "{exchange.data}" }
                    }
                }
                div { class: "notice", margin_top: "16px", strong { "Sensitive card data" } "Raw exchanges remain local unless you export them." }
            }
            div { class: "dialog-footer",
                button { class: "button", onclick: move |_| state.write().show_apdus = false, "Close" }
                button { class: "button button-primary", onclick: move |_| { state.write().show_apdus = false; open_export(&mut state.write(), ExportKind::Trace); }, "Export trace" }
            }
        }
    }
}

#[component]
fn ExportDialog(state: Signal<DesktopState>, controller: DesktopController) -> Element {
    rsx! {
        DialogFrame { title: "Export analysis", subtitle: "Choose an explicit local destination.", wide: false, onclose: move |()| state.write().show_export = false,
            div { class: "dialog-body",
                span { class: "section-label", "Content and format" }
                div { class: "option-list",
                    label { class: "radio-option",
                        input { r#type: "radio", name: "export", checked: state.read().export_kind == ExportKind::Document, onchange: move |_| open_export(&mut state.write(), ExportKind::Document) }
                        span { strong { "Result document - JSON" } small { "Versioned native schema with stable node IDs." } }
                    }
                    label { class: "radio-option",
                        input { r#type: "radio", name: "export", checked: state.read().export_kind == ExportKind::Trace, onchange: move |_| open_export(&mut state.write(), ExportKind::Trace) }
                        span { strong { "Trace - Raw native JSON" } small { "Faithful replay with sensitive APDU data. Disconnect before export." } }
                    }
                }
                if state.read().export_kind == ExportKind::Trace {
                    div { class: "notice notice-danger", strong { "Sensitive file" } "Store this raw trace securely. Redaction is not applied." }
                }
                label { class: "section-label", r#for: "destination", "Destination" }
                div { class: "destination",
                    input { class: "text-input", id: "destination", value: "{state.read().export_destination}", oninput: move |event| state.write().export_destination = event.value() }
                    button { class: "button", onclick: { let controller = controller.clone(); move |_| controller.select_export_destination() }, "Browse" }
                }
            }
            div { class: "dialog-footer",
                button { class: "button", onclick: move |_| state.write().show_export = false, "Cancel" }
                button { class: "button button-primary", onclick: move |_| controller.export(&mut state.write()), "Export" }
            }
        }
    }
}

#[component]
fn InteractionDialog(
    state: Signal<DesktopState>,
    interaction_text: Signal<String>,
    controller: DesktopController,
) -> Element {
    let Some(request) = state.read().interaction.clone() else {
        return rsx! {};
    };
    let id = request.id.clone();
    let prompt = match &request.kind {
        InteractionKind::Question { prompt }
        | InteractionKind::TextInput { prompt, .. }
        | InteractionKind::SecretInput { prompt }
        | InteractionKind::FileSelection { prompt, .. } => prompt.clone(),
    };
    if let InteractionKind::TextInput {
        default: Some(default),
        ..
    } = &request.kind
        && interaction_text.read().is_empty()
    {
        interaction_text.set(default.clone());
    }
    rsx! {
        DialogFrame { title: "Analyser request", subtitle: "The worker waits for this correlated response or cancellation.", wide: false, onclose: move |()| {},
            div { class: "dialog-body",
                p { "{prompt}" }
                match request.kind.clone() {
                    InteractionKind::TextInput { .. } => rsx! {
                        input { class: "text-input", value: "{interaction_text}", oninput: move |event| interaction_text.set(event.value()) }
                    },
                    InteractionKind::SecretInput { .. } => rsx! {
                        input { class: "text-input", r#type: "password", autocomplete: "off", value: "{interaction_text}", oninput: move |event| interaction_text.set(event.value()) }
                        p { class: "field-note", "Secret values never enter application logs." }
                    },
                    InteractionKind::FileSelection { must_exist, .. } => rsx! {
                        button { class: "button", onclick: { let controller = controller.clone(); move |_| controller.select_interaction_file(must_exist) }, "Choose file" }
                        p { class: "field-note", "{state.read().interaction_file.as_ref().map_or_else(|| \"No file selected\".into(), |path| path.display().to_string())}" }
                    },
                    InteractionKind::Question { .. } => rsx! {},
                }
            }
            div { class: "dialog-footer",
                match request.kind {
                    InteractionKind::Question { .. } => rsx! {
                        button { class: "button", onclick: answer(controller.clone(), state, id.clone(), InteractionResponse::Bool(false)), "No" }
                        button { class: "button button-primary", onclick: answer(controller.clone(), state, id, InteractionResponse::Bool(true)), "Yes" }
                    },
                    InteractionKind::FileSelection { .. } => rsx! {
                        button { class: "button button-primary", disabled: state.read().interaction_file.is_none(), onclick: move |_| {
                            let path = { state.read().interaction_file.clone() };
                            if let Some(path) = path {
                                let response = InteractionResponse::File(path);
                                controller.answer(AnswerInteraction { request: id.clone(), response }, &mut state.write());
                            }
                        }, "Use selected file" }
                    },
                    kind @ (InteractionKind::TextInput { .. } | InteractionKind::SecretInput { .. }) => rsx! {
                        button { class: "button button-primary", onclick: move |_| {
                            let text = interaction_text.read().clone();
                            let response = if matches!(kind, InteractionKind::SecretInput { .. }) { InteractionResponse::Secret(text) } else { InteractionResponse::Text(text) };
                            controller.answer(AnswerInteraction { request: id.clone(), response }, &mut state.write());
                            interaction_text.set(String::new());
                        }, "Submit" }
                    },
                }
            }
        }
    }
}

#[component]
fn DialogFrame(
    title: String,
    subtitle: String,
    wide: bool,
    onclose: EventHandler<()>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "dialog-backdrop", role: "presentation",
            section { class: if wide { "dialog dialog-wide" } else { "dialog" }, role: "dialog", aria_modal: "true", aria_label: "{title}",
                div { class: "dialog-header",
                    div { h2 { "{title}" } p { "{subtitle}" } }
                    button { class: "dialog-close", aria_label: "Close", onclick: move |_| onclose.call(()), "x" }
                }
                {children}
            }
        }
    }
}

fn answer(
    controller: DesktopController,
    mut state: Signal<DesktopState>,
    request: cardpeek_app::InteractionId,
    response: InteractionResponse,
) -> impl FnMut(dioxus::events::MouseEvent) {
    move |_| {
        controller.answer(
            AnswerInteraction {
                request: request.clone(),
                response: response.clone(),
            },
            &mut state.write(),
        );
    }
}

fn open_export(state: &mut DesktopState, kind: ExportKind) {
    state.export_kind = kind;
    state.export_destination = match kind {
        ExportKind::Document => "atr-result.json",
        ExportKind::Trace => "cardpeek-trace.json",
    }
    .into();
    state.show_export = true;
}

fn progress_percent(progress: Option<&Progress>) -> u64 {
    match progress {
        Some(Progress::Determinate { completed, total }) if *total > 0 => {
            completed.saturating_mul(100) / total
        }
        Some(Progress::Indeterminate) => 25,
        _ => 0,
    }
}

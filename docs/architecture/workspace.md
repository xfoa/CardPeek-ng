# Workspace and components

## Workspace layout

```text
Cargo.toml
crates/
  cardpeek-core/
  cardpeek-transport/
  cardpeek-pcsc/
  cardpeek-replay/
  cardpeek-analyzer/
  cardpeek-lua/
  cardpeek-app/
  cardpeek-cli/
  cardpeek-desktop/
tests/
  fixtures/
    traces/
    documents/
    analyzers/
```

The workspace manifest owns shared dependency versions and lint settings.

## cardpeek-core

This crate defines:

- APDU command and response types.
- ISO 7816 command cases.
- Status words and status descriptions.
- ATR values and reader information.
- Byte strings with explicit bit widths.
- Result documents, nodes, values, and attributes.
- Analyzer metadata.
- Trace records and documents.
- Typed domain errors.

The result document preserves ordered children and extension attributes.
Text and byte strings remain distinct.
The document does not contain UI types or encoded value prefixes.

## cardpeek-transport

This crate defines the blocking transport contract.
It includes reader discovery, connection, disconnection, reset, ATR retrieval, transmission, and capabilities.

A session explicitly reports its protocol and capabilities.
Response buffers have configurable and safe limits.
The contract supports extended responses.

Native operations return structured errors.
They do not hide errors behind status 6FFF.

## cardpeek-pcsc

This crate implements the transport contract with the pcsc crate.

It provides:

- Reader enumeration.
- Card insertion handling.
- Shared or exclusive connection as appropriate.
- T=0 and T=1 sessions.
- Cold and warm reset behavior.
- Short and extended APDU transmission.
- Structured PC/SC errors.
- Cooperative cancellation through a separate control context where supported.

Hardware integration tests must be separate from the default offline test suite.

## cardpeek-replay

This crate provides:

- Versioned native JSON trace decoding and encoding.
- Reset record handling.
- Ordered command matching.
- Response playback.
- Trace recording.
- Useful errors for missing or mismatched exchanges.

Trace files preserve exact command bytes, response data, status words, reset modes, and ATR values.
Trace decoding rejects unsupported schema versions and duplicate record identifiers.

## cardpeek-analyzer

This crate defines:

- Analyzer metadata.
- Static analyzer discovery.
- Analyzer execution requests.
- Analyzer execution results.
- A runtime-independent execution contract.

Discovery parses supported metadata headers without executing Lua.

## cardpeek-lua

This crate embeds vendored Lua 5.4 through mlua.
It implements the native CardPeek NG Lua API.
It does not provide legacy CardPeek globals, Lua 5.2 emulation, or compatibility shims.

Each analyzer run receives:

- A fresh Lua state.
- A fresh result document.
- An explicit script and resource root.
- A transport session handle.
- An interaction client.
- A cancellation token.
- A bounded log and event sink.

Global changes remain visible within a run.
They do not contaminate later runs.

## cardpeek-app

This crate owns application orchestration.

It provides:

- Reader and analyzer catalogs.
- Session lifecycle management.
- Analyzer run lifecycle management.
- Command validation.
- Ordered application events.
- Cancellation.
- Interaction correlation.
- Document snapshots.
- Trace and document export.

The CLI and Dioxus application depend on this crate.
They do not directly own PC/SC or Lua state.

## cardpeek-cli

The CLI provides non-interactive commands for:

- Reader discovery.
- Analyzer discovery.
- Card information.
- APDU transmission.
- Analyzer execution.
- Document export.
- Trace capture and replay.

Interactive prompts use the same interaction protocol as the desktop frontend.

## cardpeek-desktop

The desktop application uses Dioxus.

Its first release includes:

- Reader selection.
- Connection status.
- Analyzer discovery and selection.
- Analyzer progress and logs.
- Result tree navigation and search.
- Question and text-input dialogs.
- File selection.
- Document and trace export.

Blocking PC/SC and Lua work never runs on the Dioxus UI executor.

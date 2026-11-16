# CardPeek-ng

CardPeek-ng is a tool for reading and analysing smart cards, inspired by [CardPeek](https://github.com/l1l1/cardpeek).
It will provide a reusable engine, a command-line application, and a desktop application.
It will support PC/SC readers, Lua analysers, and replay of CardPeek-ng JSON traces.
It will not provide compatibility with CardPeek Lua analysers, XML documents, or trace captures.

The application core is written in Rust, and the UI uses Dioxus.
The workspace includes core domain types, versioned JSON documents and traces, trace replay, and native PC/SC transport.
The application engine provides bounded command and event channels, dedicated session workers, interaction correlation, deterministic cancellation, and native JSON exports.
The CLI and desktop application run Lua analysers through native PC/SC readers or replay.
Both frontends support APDU inspection and raw or sanitised trace exports.

## Analyser library

The CLI and desktop application share a user analyser library.
Both applications create the directory when it is absent.
First-time creation copies every Lua analyser from the repository `analysers` directory into the library.
The build embeds these scripts in the analyser library code.
No fixed filename list is used.
An existing library is not reseeded.
Removed bundled analysers therefore stay removed.

The library is `cardpeek-ng/analysers` under the platform configuration directory:

- Linux: `$XDG_CONFIG_HOME/cardpeek-ng/analysers`, or `$HOME/.config/cardpeek-ng/analysers`.
- macOS: `$HOME/Library/Application Support/cardpeek-ng/analysers`.
- Windows: the Roaming AppData `cardpeek-ng/config/analysers` directory.

Import additional analysers from another directory:

```sh
cargo run -p cardpeek-cli -- analysers import /path/to/analysers
```

List or remove installed analysers:

```sh
cargo run -p cardpeek-cli -- analysers list
cargo run -p cardpeek-cli -- analysers remove atr
```

Directory imports include valid Lua files directly inside the source directory.
The desktop Import Lua files action selects one or more individual files from any directory.
Imports validate metadata without executing Lua.
They reject duplicate analyser IDs and existing destination files instead of replacing them.
Imported scripts remain trusted code.

## Desktop application

Start the desktop application:

```sh
cargo run -p cardpeek-desktop
```

The application discovers native readers, the replay source, and installed analysers on startup.
Use Import Lua files to add analysers and Remove selected to remove the selected analyser.
Select a reader, an analyser, then Connect and Run analyser.
Use Send APDU to transmit exact hexadecimal command bytes.
The inspector shows progress, logs, APDU exchanges, and a searchable result tree.
The export dialog writes documents and raw or sanitised traces to an explicit destination.
Disconnect before exporting the session trace.
Native file dialogs run outside the Dioxus UI executor.
The desktop UI uses Dioxus Blitz with WGPU and Winit.
It does not use an OS WebView.

An optional argument selects a different replay trace:

```sh
cargo run -p cardpeek-desktop -- tests/fixtures/traces/atr-synthetic-001.json
```

On Debian or Ubuntu, desktop builds also require GTK 3 development files for native file dialogs.

## CLI real-reader workflow

List native PC/SC readers:

```sh
cargo run -p cardpeek-cli -- readers
```

Run the ATR analyser against a real card:

```sh
cargo run -p cardpeek-cli -- run \
  "Exact reader name" \
  atr \
  --output /tmp/atr-result.json \
  --trace-output /tmp/atr-trace.json \
  --trace-mode raw
```

Use `--trace-mode sanitised` to redact ATR values, command data, and response data.
Review sanitised traces before sharing them because redaction cannot remove every card-specific secret.

Transmit one safe command and inspect its exact exchange:

```sh
cargo run -p cardpeek-cli -- transmit \
  "Exact reader name" \
  00A40000023F00 \
  --trace-output /tmp/apdu-trace.json
```

## CLI replay

Discover analysers without executing their scripts:

```sh
cargo run -p cardpeek-cli -- discover analysers
```

Run the ATR analyser and export its JSON document:

```sh
cargo run -p cardpeek-cli -- replay \
  tests/fixtures/traces/atr-synthetic-001.json \
  atr \
  --output /tmp/atr-result.json
```

The export must match `tests/fixtures/documents/atr-synthetic-001.json`.

Run the GSM SIM and USIM analyser against its synthetic trace:

```sh
cargo run -p cardpeek-cli -- replay \
  tests/fixtures/traces/gsm-usim-synthetic-001.json \
  gsm-usim \
  --output /tmp/gsm-usim-result.json
```

The replay and real-reader commands use the user library by default.
Use `--analysers DIRECTORY` to run against a separate development or fixture directory.

The analyser selects supported files and decodes transparent files and records.
It never authenticates, changes card security state, or reads sensitive key files.
Access-denied files are reported without a PIN prompt.
Its export must match `tests/fixtures/documents/gsm-usim-synthetic-001.json`.

Analyser scripts are trusted code.
They are not sandboxed.

## Build and test

Install Rust 1.100 or a compatible newer toolchain.
Install the PC/SC development files required by the `pcsc` crate.
On Debian or Ubuntu, install `libpcsclite-dev`, `pkg-config`, and `libgtk-3-dev`.
Run the local checks from the workspace root:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
```

The default test suite uses synthetic fixtures and does not require a card reader.
PC/SC hardware tests are separate and opt-in.
See [`docs/hardware-tests.md`](docs/hardware-tests.md).

## Security and licensing

Lua analysers are trusted code.
They are not sandboxed.
Card data, APDU logs, and raw traces are sensitive by default.

CardPeek-ng is licensed under GPL-3.0-or-later.
See [`LICENSE`](LICENSE) and [`NOTICE.md`](NOTICE.md).
Fixture rules are in [`tests/fixtures/README.md`](tests/fixtures/README.md).

---

[![CI](https://github.com/xfoa/CardPeek-ng/actions/workflows/ci.yml/badge.svg)](https://github.com/xfoa/CardPeek-ng/actions/workflows/ci.yml)

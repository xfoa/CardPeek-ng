# Cardpeek-ng

Cardpeek-ng is a tool for reading and analysing smart cards, inspired by [Cardpeek](https://github.com/l1l1/cardpeek).
It will provide a reusable engine, a command-line application, and a desktop application.
It will support PC/SC readers, Lua analysers, and replay of Cardpeek-ng JSON traces.
It will not provide compatibility with Cardpeek Lua analysers, XML documents, or trace captures.

The application core is written in Rust, and the desktop UI uses GPUI.
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

The application discovers native readers and installed analysers on startup.
The navigation rail selects four independent workspaces:
Reader, Analysis, Trace, and Library.
Navigation does not connect or disconnect hardware.

Reader manages the physical PC/SC reader and current card.
Use Send APDU to transmit exact hexadecimal command bytes.
Reader shows the current capture and session exchanges.
Export the current capture without disconnecting.

Analysis uses either the current physical card or one reset-delimited segment from an open trace.
Live analysis requires a connected PC/SC reader with a card.
A trace segment supports explicit decoding or replay.
Decoding requires a compatible captured-data bundle.
Replay never uses hardware or records outside the selected segment.
Analysis shows progress, logs, APDU exchanges, and a searchable result tree.

Trace opens, searches, filters, and exports a trace without using a reader.
Use Decode in Analysis to select a segment and choose an available operation.
Trace loading does not create a replay session or start an analyser.
An optional argument opens a trace in this workspace:

```sh
cargo run -p cardpeek-desktop -- tests/fixtures/traces/atr-synthetic-001.json
```

On Debian or Ubuntu, desktop builds also require GTK 3 development files for native file dialogs.

The application icon is [`resources/icon.png`](resources/icon.png).
On X11, the application sets the window icon directly when it starts.
No installation is needed for this path.
On Wayland, install the icon and desktop entry to enable it in the application launcher and window switcher:

```sh
XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
install -Dm644 resources/icon.png "$XDG_DATA_HOME/icons/hicolor/256x256/apps/cardpeek-ng.png"
install -Dm644 resources/cardpeek-ng.desktop "$XDG_DATA_HOME/applications/cardpeek-ng.desktop"
gtk-update-icon-cache -f -t "$XDG_DATA_HOME/icons/hicolor"
update-desktop-database "$XDG_DATA_HOME/applications"
```

Restart the application after installation.
If GNOME Shell still shows the old icon, restart the shell or log out and back in.

The desktop entry expects `cardpeek-desktop` on `PATH`.
On Windows, the build script embeds the equivalent icon in the executable.

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

Cardpeek-ng is licensed under GPL-3.0-or-later.
See [`LICENSE`](LICENSE) and [`NOTICE.md`](NOTICE.md).
Fixture rules are in [`tests/fixtures/README.md`](tests/fixtures/README.md).

---

[![CI](https://github.com/xfoa/Cardpeek-ng/actions/workflows/ci.yml/badge.svg)](https://github.com/xfoa/Cardpeek-ng/actions/workflows/ci.yml)

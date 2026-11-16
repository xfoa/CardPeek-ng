# CardPeek-ng

CardPeek-ng is a tool for reading and analysing smart cards, inspired by [CardPeek](https://github.com/l1l1/cardpeek).
It will provide a reusable engine, a command-line application, and a desktop application.
It will support PC/SC readers, Lua analysers, and replay of CardPeek-ng JSON traces.
It will not provide compatibility with CardPeek Lua analysers, XML documents, or trace captures.

The application core is written in Rust, and the UI uses Dioxus.
The workspace includes core domain types, versioned JSON documents and traces, trace replay, and native PC/SC transport.
The application engine provides bounded command and event channels, dedicated session workers, interaction correlation, deterministic cancellation, and native JSON exports.
The CLI runs Lua analysers through replay.
The desktop application provides the same replay workflow in a Dioxus inspector.

## Desktop replay

Run the ATR analyser against the bundled synthetic trace:

```sh
cargo run -p cardpeek-desktop
```

The application discovers readers and analysers on startup.
Select Connect, then Run analyser.
The inspector shows progress, logs, APDU exchanges, and a searchable result tree.
The export dialog writes native JSON documents to an explicit destination.
Disconnect before exporting the raw session trace.
Native file dialogs run outside the Dioxus UI executor.
The desktop UI uses Dioxus Blitz with WGPU and Winit.
It does not use an OS WebView.

Optional arguments select a different trace and analyser directory:

```sh
cargo run -p cardpeek-desktop -- \
  tests/fixtures/traces/atr-synthetic-001.json \
  tests/fixtures/analysers
```

On Debian or Ubuntu, desktop builds also require GTK 3 development files for native file dialogs.

## CLI replay

Discover analysers without executing their scripts:

```sh
cargo run -p cardpeek-cli -- discover tests/fixtures/analysers
```

Run the ATR analyser and export its JSON document:

```sh
cargo run -p cardpeek-cli -- replay \
  tests/fixtures/traces/atr-synthetic-001.json \
  tests/fixtures/analysers \
  atr \
  --output /tmp/atr-result.json
```

The export must match `tests/fixtures/documents/atr-synthetic-001.json`.

Run the GSM SIM and USIM analyser against its synthetic trace:

```sh
cargo run -p cardpeek-cli -- replay \
  tests/fixtures/traces/gsm-usim-synthetic-001.json \
  tests/fixtures/analysers \
  gsm-usim \
  --output /tmp/gsm-usim-result.json
```

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

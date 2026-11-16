# CardPeek-NG

CardPeek-NG is a tool for reading and analysing smart cards, inspired by [CardPeek](https://github.com/l1l1/cardpeek).
It will provide a reusable engine, a command-line application, and a desktop application.
It will support PC/SC readers, Lua analysers, and replay of CardPeek-NG JSON traces.
It will not provide compatibility with CardPeek Lua analysers, XML documents, or trace captures.

The application core is written in Rust, and the UI uses Dioxus.
The workspace includes core domain types, versioned JSON documents and traces, trace replay, and native PC/SC transport.
The application engine provides bounded command and event channels, dedicated session workers, interaction correlation, deterministic cancellation, and native JSON exports.
The CLI runs Lua analysers through replay.
The desktop application remains in development.

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
Analyser scripts are trusted code.
They are not sandboxed.

## Build and test

Install Rust 1.100 or a compatible newer toolchain.
Install the PC/SC development files required by the `pcsc` crate.
On Debian or Ubuntu, install `libpcsclite-dev` and `pkg-config`.
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

CardPeek-NG is licensed under GPL-3.0-or-later.
See [`LICENSE`](LICENSE) and [`NOTICE.md`](NOTICE.md).
Fixture rules are in [`tests/fixtures/README.md`](tests/fixtures/README.md).

---

[![CI](https://github.com/xfoa/CardPeek-ng/actions/workflows/ci.yml/badge.svg)](https://github.com/xfoa/CardPeek-ng/actions/workflows/ci.yml)

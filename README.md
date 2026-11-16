# CardPeek NG

CardPeek NG is a tool for reading and analysing smart cards, inspired by [CardPeek](https://github.com/l1l1/cardpeek).
It will provide a reusable engine, a command-line application, and a desktop application.
It will support PC/SC readers, native Lua analyzers, and replay of native CardPeek NG JSON traces.
It will not provide compatibility with legacy CardPeek Lua analyzers, XML documents, or trace captures.

The application core is written in Rust, and the UI uses Dioxus.
Milestones 1, 2, and 3 are implemented.
The workspace includes core domain types, versioned JSON documents and traces, trace replay, and native PC/SC transport.
Analyzer execution and user interfaces remain in development.

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

Lua analyzers are trusted code.
They are not sandboxed.
Card data, APDU logs, and raw traces are sensitive by default.

CardPeek NG is licensed under GPL-3.0-or-later.
See [`LICENSE`](LICENSE) and [`NOTICE.md`](NOTICE.md).
Fixture rules are in [`tests/fixtures/README.md`](tests/fixtures/README.md).

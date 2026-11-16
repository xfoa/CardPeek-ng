# CardPeek NG

CardPeek NG is a tool for reading and analysing smart cards, inspired by [CardPeek](https://github.com/l1l1/cardpeek).
It will provide a reusable engine, a command-line application, and a desktop application.
It will support PC/SC readers and replay of legacy CardPeek traces.

The application core is written in Rust, and the UI uses Dioxus.
The implementation is at the workspace baseline stage.
The crates do not yet provide reader, analyzer, or document functionality.

## Build and test

Nothing to build and test yet. :)

The default test suite must not require a card reader.
Hardware tests will be separate and opt-in.

## Security and licensing

Legacy analyzers are trusted code.
They are not sandboxed.
Card data, APDU logs, and raw traces are sensitive by default.

CardPeek NG is licensed under GPL-3.0-or-later.
See [`LICENSE`](LICENSE) and [`NOTICE.md`](NOTICE.md).
Fixture rules are in [`tests/fixtures/README.md`](tests/fixtures/README.md).

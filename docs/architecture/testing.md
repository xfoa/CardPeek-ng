# Testing strategy

## Offline tests

- Core unit tests.
- APDU case and status tests.
- Native trace schema and validation tests.
- Replay transport tests.
- Native JSON document and trace round-trip tests.
- Native Lua API tests.
- Tests that legacy CardPeek globals and compatibility shims are absent.
- Analyzer integration tests with synthetic traces.
- Command, event, cancellation, and interaction tests.
- CLI snapshot tests.
- Dioxus component and application-state tests.

## Hardware tests

- Reader enumeration on supported desktop platforms.
- Card insertion and removal.
- T=0 and T=1 sessions.
- Reset behavior.
- Short and extended APDUs.
- Cancellation and disconnect behavior.

Hardware tests require explicit fixtures and are not part of the default offline suite.

## Analyzer test evidence

Store synthetic traces and expected documents as versioned fixtures.
Record the analyzer version and fixture provenance.
Do not commit live payment, identity, passport, health, transport, or SIM secrets.

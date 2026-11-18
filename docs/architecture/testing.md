# Testing strategy

## Offline tests

- Core unit tests.
- APDU case and status tests.
- Native trace schema and validation tests.
- Replay transport tests.
- Native JSON document and trace round-trip tests.
- Lua API tests.
- Tests that Cardpeek globals and compatibility shims are absent.
- Analyser integration tests with synthetic traces.
- Command, event, cancellation, and interaction tests.
- CLI snapshot tests.
- Desktop application-state and workflow tests.

## Hardware tests

- Reader enumeration on supported desktop platforms.
- Card insertion and removal.
- T=0 and T=1 sessions.
- Reset behaviour.
- Short and extended APDUs.
- Cancellation and disconnect behaviour.

Hardware tests require explicit fixtures and are not part of the default offline suite.

## Analyser test evidence

Store synthetic traces and expected documents as versioned fixtures.
Record the analyser version and fixture provenance.
Do not commit live payment, identity, passport, health, transport, or SIM secrets.

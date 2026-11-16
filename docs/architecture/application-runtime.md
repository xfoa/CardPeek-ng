# Application runtime

## Command and event model

Commands carry a command identifier and, where relevant, a session or run identifier.

The initial command families are:

- Enumerate readers.
- Connect.
- Disconnect.
- Reset.
- Transmit an APDU.
- Discover analyzers.
- Run an analyzer.
- Cancel an operation.
- Answer an interaction.
- Export a document.
- Export a trace.

Events include:

- Session state changed.
- APDU exchanged.
- Analyzer log emitted.
- Document revision published.
- Interaction requested.
- Operation progressed.
- Operation completed.
- Operation failed.
- Operation cancelled.

Every interaction has a unique request identifier.
Only the matching response can complete that interaction.

Channels are bounded.
The application defines overflow behavior for logs and document notifications.
Completion, failure, and interaction messages must not be silently discarded.

## Worker and cancellation model

A dedicated operating-system thread owns each active session.
It owns the transport and active Lua state.

The worker performs blocking operations.
The application engine sends commands and receives ordered events.
Front ends do not call the worker directly.

Cancellation is cooperative.

- Lua host calls check cancellation.
- A Lua instruction hook checks cancellation during script execution.
- Interaction waits check cancellation.
- PC/SC cancellation uses a separate control context where supported.
- Disconnect and worker cleanup have defined behavior after cancellation.

The application does not force-kill the worker thread.

## Interaction model

Lua interaction calls become requests to the interaction broker.

The first release supports:

- Yes or no questions.
- Text input.
- Secret input for PINs.
- File selection.

The worker waits for a correlated response or cancellation.
The CLI renders these requests in the terminal.
Dioxus renders them as dialogs.

Script APIs never invoke terminal or Dioxus functions directly.

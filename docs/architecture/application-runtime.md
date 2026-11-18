# Application runtime

## Command and event model

Commands carry a command identifier and, where relevant, a session or run identifier.

The command families are:

- Enumerate readers.
- Connect.
- Disconnect.
- Reset.
- Transmit an APDU.
- Discover analysers.
- Run an analyser through the legacy CLI path.
- Collect an analyser from the current physical card.
- Decode a captured-data bundle from a trace segment.
- Replay an analyser against one trace segment.
- Cancel an operation.
- Answer an interaction.
- Export a document.
- Export a trace.

Events include:

- Session state changed.
- APDU exchanged.
- Analyser log emitted.
- Document revision published.
- Interaction requested.
- Operation progressed.
- Operation completed.
- Operation failed.
- Operation cancelled.

Every interaction has a unique request identifier.
Only the matching response can complete that interaction.

Channels are bounded.
A full command channel rejects new commands without blocking the caller.
A full event channel drops logs and counts them.
Document and progress notifications are coalesced and counted.
Critical events can remove droppable events or wait for capacity.
Queue closure wakes waiting producers and receivers.
Completion, failure, and interaction messages are not silently discarded while a subscription is open.

## Worker and cancellation model

A dedicated operating-system thread owns each active session.
It owns the transport and active Lua state.

The worker performs blocking operations.
The application engine sends commands and receives ordered events.
Front ends do not call the worker directly.

Live collection requires an active PC/SC session with a card.
Migrated analysers collect values and decode them during the live run.
Live-only analysers retain the legacy execution path.
Offline decode and replay use dedicated worker threads.
They do not call the session backend or change the physical session.
Replay extracts the selected reset segment and reports the analyser, segment, and missing command on a mismatch.

Cancellation is cooperative.

- Lua host calls check cancellation.
- A Lua instruction hook checks cancellation during script execution.
- Interaction waits check cancellation.
- PC/SC cancellation uses a separate control context where supported.
- Native exchanges are recorded before a cancellation result is reported.
- Ordinary command cancellation does not disconnect the session.
- Disconnect and worker cleanup have defined behaviour after cancellation.
- Each operation reaches at most one terminal state.

The application does not force-kill the worker thread.
Shutdown closes subscriptions before joining workers.
This wakes blocked event producers.
Native session cleanup finishes before the disconnected event and trace snapshot are published.

## Interaction model

Lua interaction calls become requests to the interaction broker.

The first release supports:

- Yes or no questions.
- Text input.
- Secret input for PINs.
- File selection.

The worker waits for a correlated response or cancellation.
The CLI renders these requests in the terminal.
GPUI renders them as dialogs.

Script APIs never invoke terminal or GPUI functions directly.

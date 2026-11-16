# PC/SC hardware tests

Hardware tests are separate from the offline suite.
They require a supported PC/SC reader and a disposable test card.
Do not use a payment, identity, passport, health, transport, or SIM card that contains live secrets.

## Prerequisites

Install the PC/SC service and development files.

On Debian or Ubuntu, install `pcscd`, `libpcsclite-dev`, and `pkg-config`.
On Windows, use the built-in Smart Card service.
On macOS, use the built-in PCSC framework.

Insert the test card.
Find its exact reader name with the platform PC/SC tools.
Set `CARDPEEK_PCSC_READER` to that name.

The default short command selects the ISO 7816 master file.
Set `CARDPEEK_PCSC_SHORT_APDU` to another safe hexadecimal command if the card does not support that command.
The transport returns card status words as responses.
A non-success status word does not cause a test failure.

Run the lifecycle, cold reset, warm reset, and short APDU test:

```sh
cargo test -p cardpeek-pcsc --test hardware session_lifecycle_reset_and_short_apdu -- --ignored --nocapture
```

Set `CARDPEEK_PCSC_EXTENDED_APDU` to a safe extended hexadecimal command.
Run the extended APDU test:

```sh
cargo test -p cardpeek-pcsc --test hardware extended_apdu -- --ignored --nocapture
```

## Cancellation

Select an empty reader with `CARDPEEK_PCSC_EMPTY_READER`.
The cancellation test waits for card insertion on one thread.
It cancels that wait through a separate control context on another thread.

```sh
cargo test -p cardpeek-pcsc --test hardware cancels_card_wait_and_cleans_up -- --ignored --nocapture
```

PC/SC does not guarantee cancellation of an active card transmission on every platform.
Session calls check cancellation before and after native operations.
Do not force-kill the thread that owns the session.

## Manual removal and sharing checks

Run the lifecycle test.
Remove the card before the short command.
Confirm that the next operation returns a typed `CardRemoved` error.
Reconnect and confirm that the session recovers through a new connection.

Hold an incompatible exclusive connection with another PC/SC client.
Confirm that a new connection returns a typed `SharingViolation` error.
Disconnect CardPeek NG and confirm that the other client can connect.

Repeat discovery with no card, with an inserted card, and after reader removal.
Confirm that discovery reports the current card-presence state.
Run these checks for T=0 and T=1 cards where the available hardware supports both protocols.

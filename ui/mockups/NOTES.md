# Focused workspace prototype

## Question

Can separate reader, analysis, and settings screens reduce UI density
without hiding the current session state?

Open `cardpeek-focused-workspace-prototype.html` directly in a browser.
The previous `cardpeek-desktop-prototype.html` remains unchanged.
This prototype uses synthetic in-memory state.
It does not access a reader or modify the real analyser library.

## Layout options

Use the bottom switcher or the left and right arrow keys.
The `variant` URL parameter preserves the selected option.

- `A`: Persistent sidebar. Maximum navigation context.
- `B`: Top tabs. Minimum navigation weight and a conventional workspace.
- `C`: Compact rail. Maximum analysis width and reduced visual weight.

## Proposed screen split

The reader screen owns source selection, connection, card presence,
basic card identity, replay import, and trace export.
The workbench owns analyser selection, execution, document exploration,
decoded values, APDU tools, and activity.
Settings owns the analyser library and detection policy.

The selected node has a large human-readable value area.
Raw bytes and document identifiers remain visible below that value.
The tools use a single collapsible-looking region instead of multiple permanent panes.

## Detection model

Keep detection outside Lua execution.
Add optional static analyser metadata for ATR patterns,
application identifiers, and card families.
The application can read these declarations during discovery,
as it already does for identity and version metadata.
It can then rank matching analysers without running untrusted code
or issuing card commands.

For live cards, optional safe probes can resolve low-confidence ATR matches.
Each probe must be an explicitly declared read-only SELECT command.
The UI must show the matched evidence and confidence.
It must also permit manual analyser selection.
Replay data can use trace evidence instead of sending new commands.

ATR patterns alone are weak evidence.
A fallback analyser must remain available when no confident match exists.
Multiple equal matches need a chooser rather than arbitrary selection.

## Verdict

Pending review.
Choose the preferred navigation structure and any parts worth combining.
Delete or absorb this prototype after that decision.

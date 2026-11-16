# Security and privacy boundaries

Lua analyzers are trusted code in the first release.
They can access files and operating-system facilities exposed by their Lua environment.
The application must not describe this execution mode as sandboxed.
The UI must communicate the trust requirement for third-party scripts.

PIN entry uses secret controls.
Secret buffers are zeroized where practical.
PIN values never enter application logs.

Trace handling provides two explicit modes:

- Sanitized export for sharing.
- Raw capture for faithful replay.

Redaction cannot guarantee removal of every card-specific secret.
Raw traces remain sensitive files.
Exports require an explicit destination.

The first release contains no telemetry and no automatic update mechanism.

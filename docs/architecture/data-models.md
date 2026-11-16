# Native data models

## Native result model

The native document model includes:

- A document identifier.
- A revision number.
- Stable run-scoped node identifiers.
- Ordered child nodes.
- A parent relationship.
- Class, label, and identifier attributes.
- Size, value, alternative value, and MIME type.
- Extension attributes required by analysers.

Byte strings retain their bit width.

Native document export uses a versioned JSON schema.

## Native trace model

Native trace export uses a versioned JSON schema.
Every record has a stable identifier and an explicit kind.
Reset records contain a reset mode and ATR.
Command records contain exact command bytes, response data, and a status word.
Trace decoding rejects malformed JSON, unsupported schema versions, and duplicate record identifiers.

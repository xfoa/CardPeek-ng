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

Reset records delimit zero-based trace segments.
Commands before the first reset belong to segment zero.
Segment extraction preserves the original record identifiers.

Schema 1 remains supported for existing traces and replay fixtures.
Schema 2 adds optional captured-data bundles.
A bundle identifies its analyser, analyser version, capture schema, and source segment.
Each captured value has a name, native bytes, and a source record identifier.
Validation requires the source record to belong to the declared segment.
Live ATR collection and offline ATR decoding produce equivalent results through this model.
Sanitised export removes captured bytes as well as ATR and APDU data.

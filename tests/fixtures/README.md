# Test fixture policy

Fixtures support offline tests and compatibility comparisons.
They must be deterministic, versioned, and safe to publish.
Hardware access must never be required to read or use them.

## Directory layout

- `traces/` contains legacy-compatible trace captures.
- `documents/` contains native JSON and legacy XML documents.
- `analyzers/` contains analyzer scripts and resources used by tests.

Each fixture or fixture set must have an adjacent metadata file.
Use a TOML metadata file with the fields below.
Keep the metadata beside the files it describes.

```toml
schema_version = 1
fixture_id = "atr-synthetic-001"
description = "Synthetic ATR replay example"
origin = "synthetic"
created_by = "CardPeek NG contributors"
source_project = ""
source_url = ""
source_revision = ""
source_version = ""
retrieved_at = ""
license = "GPL-3.0-or-later"
contains_live_card_data = false
sanitization = "not-applicable"
```

Use ISO 8601 dates for retrieval and creation dates when a date is needed.
Use a stable fixture identifier in file names and metadata.

## Provenance

For generated fixtures, record the generator or procedure and its inputs.
For imported fixtures, record the upstream project, URL, revision, version, and retrieval date.
Record the source analyzer version for compatibility fixtures.
Preserve source notices and attribution in [`NOTICE.md`](../../NOTICE.md).

Expected documents and traces must identify the analyzer and source exchange that produced them.
Do not infer provenance from a filename alone.

## Sensitive-data rules

Never commit live payment, identity, passport, health, transport, or SIM secrets.
Never commit PINs, cryptographic keys, authentication values, tokens, or private cardholder data.
Do not add live card captures merely because they appear anonymous.
Prefer generated values and synthetic card identities.

A fixture derived from live data requires an explicit privacy review.
Its metadata must set `contains_live_card_data = true` and describe the sanitization method.
Approval must establish that publication is permitted.
When this cannot be established, replace the fixture with synthetic data.

Sanitization is not guaranteed to remove every card-specific secret.
Do not use sanitized fixtures as a substitute for source authorization.

Raw local captures must stay outside version control unless they satisfy this policy.
Use an explicit export destination.
Do not send fixtures or traces to external services.

## Review checklist

Before adding a fixture:

1. Confirm that the data is synthetic or approved for publication.
2. Confirm that all metadata fields are accurate.
3. Confirm that provenance and analyzer versions are recorded.
4. Confirm that required notices and licenses are present.
5. Run the offline tests without hardware or external network access.

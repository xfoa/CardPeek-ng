# Native Lua API contract

## Runtime and startup

The runtime loads each analyzer with `require("cardpeek")`.
The module exposes the native CardPeek NG API through one namespace.
It does not install legacy CardPeek globals or emulate legacy Lua behavior.

Analyzer discovery does not execute the analyzer.
The runtime provides explicit script and resource roots.
Module loading and file access follow the trusted-script policy.
The first release does not provide `dofile` compatibility behavior.

## API areas

| Module | Required behavior | Initial validation |
| --- | --- | --- |
| `cardpeek.bytes` | Constructors, explicit widths, indexing, iteration, slices, concatenation, comparison, conversion, and formatting | Focused unit tests and ATR fixtures |
| `cardpeek.card` | Session information, reset, ATR access, explicit APDU transmission, protocol, and capabilities | Replay integration tests |
| `cardpeek.nodes` | Root access, append, navigation, attributes, typed values, and provenance | Document conformance tests |
| `cardpeek.log` | Structured levels and messages | Bounded event tests |
| `cardpeek.ui` | Questions, text input, secret input, and file selection | Broker integration tests |
| `cardpeek.progress` | Determinate and indeterminate progress | Application event tests |

The native card API returns structured success and error values.
It does not perform implicit retries after status 6C.
It does not issue implicit GET RESPONSE commands after status 61.
Analyzers may implement those operations explicitly.

Binary values cross the Lua boundary as typed native values.
Legacy `1:`, `4:`, and `8:` prefixes remain confined to XML conversion.

Each bundled analyzer has replay fixtures and expected result documents.

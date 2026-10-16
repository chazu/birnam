# Ordinary records own their fields

2026-10-15. Follow-up to [optional Store](optional-store-and-relational.md), after
`bd8a532e` completed the coordination cleanup. This slice migrates ordinary record
fields using existing authored ObjectModel and Reflect protocols. It adds no Rust
runtime owner, language primitive, subscription or method-identity guard.

| Family | Owned named slots | Existing API retained |
| --- | --- | --- |
| Logger / Log | `name`, `level`, `sink` | Named logger defaults, threshold ranking, chainable setters and emission |
| Config | `configPath` | Discovery/refresh, nearest-parent resolution, fresh TOML reads and nil fallback |
| HttpRequest | `method`, `path`, `raw` | Request parsing, query stripping and malformed-request defaults |
| ProcResult | `stdout`, `exit` | Captured output, exit status, `ok`, and failed-start result |
| AppConfig | `values`, `origins`, `configPath` | Resolution precedence, value/origin access and selected path |

Here “records” means ordinary objects with fields, not Store facts or a new
language-level record type. A Logger object and an explicitly asserted fact about
that Logger are distinct things.

These twelve fields are ordinary reference slots. Getters, constructors and setters
send to the current ObjectModel methods. Named `slotNames` descriptions make the
same storage inspectable and editable with `Reflect slotsOf:` and
`Reflect slot:in:put:`. No read or field edit creates a fact handle or implicitly
asserts a database fact. A program can still explicitly assert facts about any
record, including under the same attribute names; those facts remain independent.
No old facts are scrubbed by this migration.

The public constructors initialize the same values and keep their existing return
conventions and send order. Logging/file/process algorithms, config precedence,
error handling and request parsing are unchanged. Source definitions remain
replaceable. A collecting/reentrant `rankOf:` replacement can change a logger's
sink while the outer level edit updates its own field; neither freezes the other
send nor replaces the whole record with a stale snapshot.

Clones now follow ordinary slot ownership: cloning copies the field references,
then replacing a field on the clone leaves the original alone. Aliases of the same
object observe the same edit. This intentionally replaces the old fact-key behavior
where a raw clone had no facts under its new identity. Shared referenced values
remain shared; this is not a deep copy or a promise of immutable application state.
AppConfig's description now says resolved snapshot, and its slots remain malleable.

## Scope and remaining work

This removes field storage from these five families. It does not make complete
module compilation/loading independent of Store: compiler metadata, documentation,
foreign-wrapper recipes and other runtime owners still participate. AppConfig is
currently a source-only module; tests load its shipped source rather than changing
bootstrap/package composition in this slice. The module/primitive sets and image
format are unchanged.

The record inventory was incomplete: AppConfig also had implicit field facts.
The plan now includes it in delivered work and explicitly records still-fact-backed
Bench/Test runner records and REPL state. Socket handles and task/isolate state
need their resource-lifetime and persistence contracts audited before migration.
Compiler metadata/source integration, Alien recipes and coordinated lifecycle state
remain the next larger structural work. Compiled startup must continue using
compilation-produced metadata without AST analysis or parse fallback; moving that
metadata must not replace Store with another permanent opaque native registry.

## Validation

- 826 language assertions and all existing postchecks pass, with the new record
  gate included in `birnam-test.sh` and its native retention check included in the
  native suite.
- Record-specific tests cover API defaults, reflected writes, aliases, independent
  clones, nil, live helper replacement with collection/reentry, and explicit facts
  under the same attribute names. Real file logging checks filtering and output;
  real process capture checks stdout and status. Config tests cover parent
  discovery, malformed TOML and missing files; a real REPL refresh agrees with
  reflective inspection. Forced source bootstrap also passes the semantic fixture.
- A native caller measures Id's raw allocation count after each authored batch
  returns and precise collection finishes. Warm-up, 0, 1, 30, and three 100-call
  batches leave zero extra objects and zero facts. An intentional retained-logger
  negative control fails at one measured call (+4 objects). No larger leak budget
  or language telemetry tolerance is introduced.
- Mapped and rebuilt images preserve all five record families, shared field
  identities, clone independence, reflected mutation and explicit facts after the
  writer source is deleted. New records can be constructed after restoration.
- The absent-owner floor executes 22 actual record method bodies alongside 17
  existing Cell/Reflect helpers. Store, Relational, Generation and Lang libraries
  are physically absent. Actual getters, Logger construction/setters, AppConfig Map
  access, cloning, reflection and reclamation work there. The floor constructs
  prototypes with the declared slot layouts; it does not claim full source or
  I/O-module bootstrap without Store.
- Lean checks five additional laws for field updates, shallow cloning, independent
  facts and reentrant edits to another field. The full gate checks 318 theorems,
  rejects 85 mutations and obtains all 199 expected TLC outcomes. No new temporal
  protocol is introduced; these abstract laws are not a proof of the whole runtime.

The build used the explicit development cache. No release, push or user installation
was performed. No CPU speedup is claimed. The measured result is elimination of
per-operation facts and retained temporary records in the tested workloads.

The [receipt](../measurements/ordinary-record-owners-20261015.json) records production
source hashes, build provenance, test logs and detailed test receipts. Harness fixes
load AppConfig through its existing source path and execute every exported record
getter in the Store-absent floor; they do not change production behavior.

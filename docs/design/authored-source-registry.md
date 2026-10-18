# Source registry owned by Birnam

2026-10-18. Implements the bounded Generation migration selected after
[one-way image application](one-way-image-application.md), baseline `ab8c36cb`.
The rest of Generation's candidate and certification implementation remains Rust.

## Ownership and interoperability

[source-registry.birnam](../../modules/source-registry.birnam) creates the
Globals-held `SourceRegistry` object. One ordinary slot contains
`#[contentRevision, orderedEntries]`; entries are `#[moduleName, compilationRecord]`
Vectors. Compilation records retain their original source and metadata objects.
The owner has no Store backing, native table, native registry seed or private ABI.

Codegen and Generation resolve the public binding through Globals and send the
owner's current messages via Id. Generation's old `Registry`, field, native
registration methods and registry root visitor are removed. Its
[sources helper](../../rust/birnam-generation/src/sources.rs) only sends messages
and reads ordinary returned values; it owns no registry state and knows no slot
layout. The same restriction applies to Image and Codegen.

Id dictionaries remain the authority for live dispatch. Changing a source record
does not install code, alter a Method or certify a world. Optional certification
can compare recorded source with current code when explicitly requested.

## Ordinary protocol

| Operation | Default behavior |
| --- | --- |
| `state` / `state:` | Inspect or explicitly replace the actual ordinary state; also discoverable through Reflect's slot protocol |
| `entries` / `entries:` | Inspect ordered entries or publish a replacement, advancing the content revision when name/record pairs change |
| `revision` | Read the content revision used by explicit certification and image observations |
| `recordFor:` | Find the compilation record for a module-name Symbol, or nil |
| `moduleSourceFor:` | Return the registered original source String, or nil |
| `compilationForSource:` | Find a record by original source identity; equal text alone does not attach metadata |
| `recordCompiledSource:named:` | Validate retained metadata, append a new declaration, or retain the first equal-text registration; return the supplied source text |
| `replaceCompiledSource:named:` | Validate and replace an existing declaration without installing its code |
| `replaceRecord:named:expected:` | Conditionally replace one selected record, preserving unrelated entries; report conflict rather than overwrite an intervening edit |
| `sourceRegistry` | Return the ordinary ordered inspection view `#[#sourceRegistryV1 #[name record] ...]` |
| `validate` | Validate the current entry structure, unique names and retained metadata through current messages |

Clones initially share the persistent state value; publishing a clone's edited
root does not edit the original. Retained snapshots share referenced objects, not
deep-frozen copies. Explicit raw state replacement is malleable and can produce
invalid state; subsequent operations may fail. No private mirror repairs it.

Duplicate source text now uses the current String `=` send. This replaces the
Rust registry's copied-byte comparison. Default equality preserves the former
result, while replacing equality has its ordinary language effect. Source-identity
lookup and selected-record comparisons use ordinary identity sends. The source
loader regression now injects collecting/reentrant equality behavior; byte-borrow
callbacks remain separately tested at the compiler/loader boundary.

A short mutation uses the existing, replaceable task-transition protocol when
available, and publishes through ObjectModel's existing conditional-slot message.
Synchronous reentry can still conflict: callbacks are not replayed. No Method
identity is guarded and no new lock or transaction service is introduced.

One ordinary content revision replaces the two native counters. A→B→A still
advances it; publishing the same name/record pairs does not. Candidates do not
use this whole-registry revision as token expiry: they compare their selected
record and selected owner. Ordinary sends do not consult a certificate.

## Candidate source writes are ordinary sends

Prepared work and a retained inverse hold their selected source owner through
ordinary GC roots. A new operation resolves the current binding; compensation
uses the receiver it actually wrote, even if a callback rebinds SourceRegistry.

The source write is recorded in the explicit candidate operation's attempted-write
journal before its outgoing send. Postconditions include the planned new record.
On failure, compensation restores the old record only if that selected owner still
contains the planned written record. A setter may fail after writing; a differing
callback write must survive. Rebinding the owner never authorizes writes to the
replacement object. Failed compensation remains a truthful reported outcome.

The seven-mode Host regression exercises ordinary Error and native failure after
the write, failure before it, collection, a conflicting selected source, an
unrelated registration, owner rebinding, and successful activation/undo with an
unrelated callback registration. The existing conditional candidate contract
remains in force. Ordinary method edits remain independent of these operations.

## Bootstrap and image representation

The early installation unit has no named-module declaration, following Cell's
precedent. It can therefore install the owner before named artifact admission
looks up a prior source. The Module manager subsequently lists it as the loaded
authored `#sourceRegistry` facility. Its methods remain inspectable ordinary
Methods; this early installation unit is not a candidate-managed named source.

Both seed-enabled builds and production bootstrap load this compiled artifact
after Error and before Module. When the prefix is empty, the existing compiler
seed compiles the authored installation source. There is no Rust implementation
of registration to retain or hand off. Missing ownership during ordinary registry
operations fails rather than selecting a hidden fallback.

Images now carry `#[#sourceRegistryV2 owner]`, referencing the same ordinary object
captured through Globals and user aliases. Codegen's existing temporary image
stage retains that identity while restored Methods are prepared. After Globals,
Methods and Maps are restored, the loader checks the visible owner identity and
sends `validate`. It does not copy another inventory into a native table or into
the restored object's slots. Cleanup only releases temporary staging roots.

The V1 **inspection view** remains supported. The old V1 **image transport**, and
images with no source-owner section, are rejected. Recreate those images from
source using the new runtime; `--rebuild` recompiles native code but does
not migrate the old registry data representation. No compatibility adapter or
source-parsing fallback is added. The artifact metadata codec remains unchanged.

Image save observes the registry owner and its content revision alongside the
existing explicit snapshot observations. A source registration during saving
rejects the mixed snapshot and preserves that callback edit. A returned validation
Error propagates without publishing a file. This is consistency checking for an
explicit snapshot, not a guard around ordinary sends or Method replacement.

Partial loading follows the selected contract. The visible source owner changes
when its Globals state is restored; a failure before that point can leave the old
owner visible. Codegen must read whichever owner is visible, without an earlier
native registry exchange or an automatic inverse. A failure after application
begins still leaves applied effects and reports partial loading.

## Qualification and limits

Qualification passes against `/tmp/birnam-source-registry/final-prefix`:

- Fresh-prefix installation build, with no native registry implementation or seed;
  installed startup with parsing and compilation disabled.
- 847 language assertions, including 21 source-owner assertions.
- All 12 Host generation checks: owner fault injection and image aliases, earlier
  lifecycle regressions, mapped/rebuilt/resaved/divergent images, console/TCP and
  62 compiled-unit policy checks.
- Native generation projection, certification, candidate transaction/retirement,
  source inspection/loading, image roundtrip, 20 successive save/load cycles with
  stable retention, snapshot admission and partial-load callback preservation.
- 386 Lean theorems, 120 rejected mutations and 260 expected TLC outcomes.
  These abstract models supplement production ownership tests, not arbitrary callbacks.
- Declared native exports unchanged; no cross-owner dynamic links introduced.

The [receipt](../measurements/authored-source-registry-20261018.json) records
commands, hashes, fixture provenance and the individual native results. Two old
fixture assumptions were updated: duplicate registration now exercises current
String equality, and partial image loading observes the visible Globals-held
registry rather than expecting an earlier native table exchange. The corrected
roundtrip and all remaining suites pass; the already-passing repeated-save test
body was unchanged and was not rerun needlessly.

An exploratory three-run sample, using fresh processes, warm filesystem caches
and precise GC, measured startup plus a trivial source script at median 4.057s
before and 4.414s after (about 9% slower). This is a modest observed startup cost,
not a general execution benchmark. Profile source validation and lookup before
larger ports; in particular, registration currently asks Codegen for a revision
value when it primarily needs retained metadata and the module name. Ordinary
message dispatch does not consult this registry.

This migration is committed with its report. No push, release or user-prefix
installation accompanies it.

The migration does not move the remainder of Generation, resume Relational, or
prove core bootstrap without Store/Relational. Candidate compensation policy is
still a separate feature decision. No permanent native service replaces the
registry, and the eventual ability to discard Rust remains the goal.

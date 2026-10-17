# Live compiled units

`CompiledUnit` prepares one module's complete successor document without changing
the running system. Activation asks the host to publish its declarative method
delta and a new whole-runtime `DispatchGeneration` together. Rollback asks the
host to republish the exact predecessor.

Compiled global references look up the current binding by name when executed.
They see globals published by later module loads and subsequent replacements;
compilation does not freeze a global's value or retain a raw Symbol address.

Every module has the same representation and is live-editable. The current
candidate contract permits declarative method changes and capture-free
top-level block bindings such as `let handler := [| ...]`. Module identity,
dependencies, and every residual nonmethod form must stay equal. Arbitrary
initializer, object-graph, and FFI changes and module introduction remain
deferred.

## Lifecycle

```text
compile -> open -> map -> validate -> activation intent -> host safe point
                                                     -> rollback intent
```

- `compile:named:output:` accepts a complete module document and writes a dylib
  plus its pointer-free `.candidate` envelope.
- `Codegen compile:output:selection:` writes the same predecessor-bound
  candidate envelope for candidate AOT delivery; open/map/activate it rather than
  passing it to eager `Codegen load:`.
- `open:` verifies the envelope, artifact checksum, platform, ABI, exact source,
  source revision, dependencies, and declarative method manifest without mapping.
- `map` crosses the trusted-native boundary but remains inert.
- `validate` checks the candidate's selected binding observations and current
  source lineage, then discards its one-shot prepared capability. It does not
  certify every managed binding.
- `activate` returns an `EvolutionIntent`. The console host fulfills it only
  between top-level forms while native quiescence is active.
- `rollback` returns the corresponding predecessor intent.

Errors are values. Rejection before commit leaves dispatch and source authority
unchanged.

A prepared token observes its target bindings and metadata; it is not a lease on
the global dictionary or Globals counter. Creating a branch Block or editing an
name outside the operation's binding/Global scope between preparation and
commitment is allowed, including a name tracked in another managed module. A replaced setter
may perform additional edits and return an ordinary success value; Generation
checks that the requested binding was actually installed or removed. Errors still
propagate and a no-op setter still fails. Commit, compensation and rollback check
the selected state, not every binding in the managed ledger. A relevant conflict
does not itself mark the generation drifted or revoke its inverse. Compilation
still requires the recorded source lineage and initial adoption; removing that
requirement is a separate source/image change.


## Complete replacement example

Assume `#pricing` is already in the current source map:

```text
let source := "[Module named: #pricing requires: #[]];
[Reflect defineMethod: #salePrice on: Product as: [:self | [[self price] * 90 / 100]]];
[Reflect removeMethod: #legacyPrice on: Product]";

let path := "/tmp/pricing-next.dylib";
[CompiledUnit compile: source named: #pricing output: path];

let unit := [CompiledUnit open: path];
[unit map];
[unit validate];
[unit activate]                  // return the intent as the top-level result
```

The REPL host recognizes that result, enters quiescence, fulfills it, exits the
guard, and prints the committed unit. A later top-level `[unit rollback]` follows
the same path.

The candidate declaration must match the existing module. Its dependency vector
and residual effectful initializer digest must equal the active revision.
Declarative method additions, replacements, and removals may differ, as may a
capture-free top-level block binding. Activation rebuilds every such binding
exactly once and journals its Globals root for compensation, drift detection,
and rollback. Duplicate bindings, captured projected blocks, indirect targets,
first-time target specialization, malformed declarations, stale bases, and
other changed top-level forms are rejected.

## One runtime-wide generation

The authored `DispatchGeneration current` / `current:` protocol owns the current
generation reference. Lifecycle records use ordinary named slots, independently
of any explicitly asserted Store facts.

The generation owns a sealed, sorted source map and its canonical digest.
`CompiledUnit activeFor:` is only a convenience derived from that map; there is no
per-module active-authority mirror. Activation replaces one source-map entry and
inherits every untouched entry. Rollback republishes the predecessor map and
restores only bindings touched by the successor.

## Dependencies and module introduction

In L1a dependencies must already be present in G0. `prepareDependencies` remains
useful during bootstrap, but ordinary AOT `Module load:` after G0 answers
`#moduleIntroductionNotAvailable`. The projected block-binding subset does not
broaden this into arbitrary initializer or migration semantics.

## Direct edits and drift

Ordinary method installation and removal remain available through the current
owner messages. There is no running-phase authorization guard. A unit’s `state`
reads its recorded lifecycle status; explicit `Generation reconcileCandidate:`
checks whether the managed revision still matches live bindings. Detected drift
can disable managed activation/rollback, but does not undo ordinary edits. Image
snapshots preserve imageable live code even when revision metadata has drifted.
The remaining whole-program restriction is being narrowed; it is not a language
requirement that user code remain unchanged.

## Images and process boundaries

An image carries the exact generation source map, its digest, and one code unit
built from every complete document once in dependency order followed by the
ordered source-backed method operations recorded by direct scripts and REPL
forms. Normal boot maps that unit only when its embedded authority matches;
`--rebuild` recompiles and maps the same accepted GenerationProjection from
recovered source and selection, then retains the same digest. A selected
projected global is reconstructed by that unit instead of being serialized as
a duplicate closure fact.

Candidate handles, native roots, and rollback pointers are process-local. The
saved `DispatchGeneration` remains durable authority, and new post-boot candidates
may create successors, but prior-process rollback capabilities do not become
durable merely because their provenance objects did.

Candidate bundles are trusted native input, not a security sandbox. `open:`
performs byte-level inspection without loading code, and `map` makes the native
boundary explicit; checksums establish identity, not authorship.

Lifecycle facts are no longer created automatically. CompiledUnit and generation
fields are discoverable through `Reflect slotsOf:` and writable through ordinary
slot reflection. Explicit facts, including facts under the same names, are separate.
Image composition does not delete old record fields or user annotations; retained
records remain historical data and unreferenced records can be collected.

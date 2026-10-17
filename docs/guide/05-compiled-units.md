# Live compiled units

`CompiledUnit` prepares one module's complete successor document without changing
the running system. Activation applies its declarative method delta and replaces
that module's source record. Conditional rollback restores the exact selected
Methods, Globals and prior source record. No adopted generation is required.

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
does not itself mark the generation drifted or revoke its inverse. Compilation uses only the registered source of the candidate's own module.


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

`CompiledUnit activeFor:` finds a retained candidate whose source record is current
for that module. Candidate activity does not create or modify a DispatchGeneration.
Different modules can be rolled back independently when their affected bindings
do not conflict. Lifecycle records remain ordinary reflected slots.

## Sources and certification

Source and AOT module loading remain available before, during and after candidate
work. Registration is never sealed. Candidate compilation checks that its own
registered module has compatible identity, dependencies and residual initializers.
An unrelated registration does not stale the candidate or get lost on rollback.
Recompile an ABI-4 candidate bundle: ABI 5's predecessor is a module revision,
not a whole-program digest.

Console, TCP and workers do not adopt automatically. Optional
`DispatchGeneration adoptCurrent` certification snapshots currently registered
source definitions; it is renewable and independent of candidate undo. Explicit
validation reports disagreement without changing status or revoking inverses.
Restoring a selected source record/binding to the same identity between conditional
checks is permitted. Images preserve actual live code regardless of certification.

## Images and process boundaries

An image restores live Methods, their captures and retained source descriptions.
Normal boot maps the recorded code; `--rebuild` reconstructs that code from the
image's canonical programs and selection. Neither path adopts a generation or
mints replacement CompiledUnits. Retained generation records, aliases and explicit
annotations survive as ordinary data.

Candidate handles, preparation tokens and inverse journals are process-local.
Loading drops them even when the saved source description exactly matches live
code. Recorded `state` and `rollbackAvailable` fields describe history; they do
not recreate an executable rollback capability. Saving and resaving do not
reconcile the ledger or alter those fields.

To explicitly establish a new baseline after restoration:

```text
let baseline := [DispatchGeneration adoptCurrent];
```

This validates the retained source/selection against live code, then creates one
predecessor-free generation with no component units. A mismatch returns an Error
and keeps the prior current record. Existing records and annotations are untouched.
A successful baseline enables explicit source certification. Candidate activation
and rollback do not require it. A later call refreshes the optional certificate from current registered source.
Use `Generation reconcileCandidate:` to validate without refreshing. Ordinary startup and worker preparation leave adoption to the caller.

Candidate bundles are trusted native input, not a security sandbox. `open:`
performs byte-level inspection without loading code, and `map` makes the native
boundary explicit; checksums establish identity, not authorship.

Lifecycle facts are no longer created automatically. CompiledUnit and generation
fields are discoverable through `Reflect slotsOf:` and writable through ordinary
slot reflection. Explicit facts, including facts under the same names, are separate.
Image restoration does not delete old record fields or user annotations; retained
records remain historical data and unreferenced records can be collected.

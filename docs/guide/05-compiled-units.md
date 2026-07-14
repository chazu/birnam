# Live compiled units

`CompiledUnit` prepares one module's complete successor document without changing
the running system. Activation asks the host to publish its declarative method
delta and a new whole-runtime `DispatchGeneration` together. Rollback asks the
host to republish the exact predecessor.

Every module has the same representation and is live-editable. The current L1a
policy permits method-only successors: module identity, dependencies, and the
derived non-method projection must stay equal. Top-level initializer/global/FFI
changes and module introduction are deferred to the later migration phase.

## Lifecycle

```text
compile -> open -> map -> validate -> activation intent -> host safe point
                                                     -> rollback intent
```

- `compile:named:output:` accepts a complete module document and writes a dylib
  plus its pointer-free `.candidate` envelope.
- `open:` verifies the envelope, artifact checksum, platform, ABI, exact source,
  source revision, dependencies, and declarative method manifest without mapping.
- `map` crosses the trusted-native boundary but remains inert.
- `validate` checks the candidate against the exact current generation and then
  discards its one-shot prepared capability.
- `activate` returns an `EvolutionIntent`. The console host fulfills it only
  between top-level forms while native quiescence is active.
- `rollback` returns the corresponding predecessor intent.

Errors are values. Rejection before commit leaves dispatch and source authority
unchanged.

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
and non-method digest must equal the active revision. Declarative method additions,
replacements, and removals may differ. Duplicate bindings, indirect targets,
first-time target specialization, malformed declarations, stale bases, and changed
top-level forms are rejected.

## One runtime-wide generation

Authority is the single-valued Store cell:

```text
entity #dispatch, attribute #activeGeneration, value DispatchGeneration
```

The generation owns a sealed, sorted source map and its canonical digest.
`CompiledUnit activeFor:` is only a convenience derived from that map; there is no
per-module active-authority mirror. Activation replaces one source-map entry and
inherits every untouched entry. Rollback republishes the predecessor map and
restores only bindings touched by the successor.

## Dependencies and module introduction

In L1a dependencies must already be present in G0. `prepareDependencies` remains
useful during bootstrap, but ordinary AOT `Module load:` after G0 answers
`#moduleIntroductionNotAvailable`. Later work adds checked initializer/migration
semantics rather than hiding arbitrary effects inside activation.

## Direct edits and drift

While the runtime is running, ordinary method installation/removal is rejected by
the native Store/Codegen guard. Candidate commits are the supported mutation path.
A raw floor mutation is unsupported corruption; reconciliation and image save
reject it rather than adopting an untracked second authority.

## Images and process boundaries

An image carries the exact generation source map, its digest, and one code unit
built from every complete document once in dependency order. Normal boot maps that
unit only when its embedded authority matches; `--rebuild` evaluates the same
documents and retains the same digest.

Candidate handles, native roots, and rollback pointers are process-local. The
saved `DispatchGeneration` remains durable authority, and new post-boot candidates
may create successors, but prior-process rollback capabilities do not become
durable merely because their provenance objects did.

Candidate bundles are trusted native input, not a security sandbox. `open:`
performs byte-level inspection without loading code, and `map` makes the native
boundary explicit; checksums establish identity, not authorship.

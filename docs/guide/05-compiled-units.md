# Staged compiled units

`CompiledUnit` lets you prepare a native method patch without changing the running
system, then publish all of its method additions, replacements, and removals as one
dispatch generation. If the live bindings have not changed afterward, that
generation can be rolled back exactly.

Use this lifecycle when inspection, atomic publication, and rollback matter. Use
ordinary source evaluation or `Module load:` when you want immediate initializer,
global, object, FFI, filesystem, or other effects.

## The lifecycle

A candidate moves through these explicit operations:

```
compile -> open -> prepare dependencies -> map -> validate -> activate -> rollback
```

- `compile:named:output:` accepts candidate-safe source and writes both a dylib and its pointer-free `.candidate` envelope. It does not open or map the result.
- `open:` verifies the envelope, complete artifact checksum, platform, ABI, source, and declarative operation list without mapping native code.
- `prepareDependencies` performs ordinary `Module load:` effects for dependencies that are still missing. It is optional when `missingDependencies` is empty.
- `map` is the explicit trusted-native boundary. It maps and mechanically verifies the candidate, but still does not publish methods.
- `validate` proves the candidate can activate over the exact live base. `activate` performs the same validation itself, so a separate call is useful as a check but is not required.
- `activate` publishes the complete operation set and lifecycle facts together.
- `rollback` restores the exact predecessor bindings while the rollback capability remains valid in the current process.

Every operation that can fail answers an `Error` value. Check `isError` before
continuing; a rejected open, map, validation, activation, or rollback does not
partially change dispatch.

## A complete patch

The source may contain one matching `Module` declaration, method definitions, and
explicit method removals:

```
let source := "[Module named: #pricePatch requires: #[]];
[Reflect defineMethod: #salePrice on: Product as: [:self | [[self price] * 90 / 100]]];
[Reflect removeMethod: #legacyPrice on: Product]";

let path := "/tmp/price-patch.dylib";
let built := [CompiledUnit compile: source named: #pricePatch output: path];

let unit := [CompiledUnit open: path];
[unit missingDependencies];       // #[]
[unit map];                       // still no dispatch change
[unit validate];                  // optional preflight
[unit activate];                  // both operations become visible together

[CompiledUnit activeFor: #pricePatch];
[unit rollback]                   // restores salePrice and legacyPrice exactly
```

In production code, stop when any answer is an error:

```
let answer := [unit activate];
[[answer isError]
  ifTrue: [| [System println: [answer message]]]
  ifFalse: [| [System println: "patch active"]]]
```

The candidate's name is a Symbol and must agree with any `Module named:` declaration
inside the source. Targets such as `Product` must already be stable named globals
with their own method-family vtable. Candidate source cannot create or specialize a
target for the first time.

## Dependencies are an explicit ordinary effect

Opening a candidate reports dependency names but never loads them. Mapping refuses
to proceed while any are missing:

```
let missing := [unit missingDependencies];
[unit prepareDependencies];
[unit map]
```

`prepareDependencies` uses normal module loading. Module initializers may therefore
change globals, objects, FFI state, or the outside world, and rejecting or rolling
back the candidate does not undo those effects. Keeping this step visible prevents
a candidate transaction from making a false promise about arbitrary module code.

## What candidate source can contain

The candidate compiler accepts this declarative subset:

- an optional matching `[Module named: #name requires: #[...]]` declaration;
- `[Reflect defineMethod: #selector on: NamedTarget as: block]`, including the `doc:` form; and
- `[Reflect removeMethod: #selector on: NamedTarget]`.

It rejects arbitrary top-level sends and object creation, global creation or
replacement, FFI operations, external effects, vtable layout/parent changes,
first-time vtable specialization, and duplicate operations for one target/selector
pair.

Those operations are not forbidden in Birnam. They remain available through normal
source and binary module loading, with immediate non-transactional semantics; they
are simply outside the staged candidate API.

## Generations, supersession, and rollback

`unitName`, `generation`, `requires`, `provides`, and `state` expose the reflected
candidate. The important states are:

- `#candidate` — inspected and eligible for mapping/activation;
- `#active` — owns the current managed dispatch generation;
- `#superseded` — replaced by a later generation and retained as its predecessor;
- `#drifted` — managed dispatch changed outside the lifecycle;
- `#rolledBack` — its inverse was applied; another rollback is an idempotent no-op;
- `#historical` — provenance remains, but it no longer owns executable dispatch.

Activating another candidate with the same name supersedes the active generation.
Bindings omitted from the newer patch are inherited, not removed. Rolling the newer
generation back republishes its predecessor and restores only the bindings touched
by the newer generation.

Rollback is deliberately conservative. Before changing anything, it verifies that
the live target identities and direct method bindings still equal the activated
post-state. If an ordinary edit intervened, rollback answers an error and marks the
generation `#drifted` instead of overwriting that edit.

## Drift and adopting live edits

An ordinary supported method edit to a binding owned by an active generation marks
that generation `#drifted` immediately. A raw floor mutation is detected when the
unit is next reconciled through `state`, `activeFor:`, another lifecycle operation,
or image save.

If every intervening edit has portable recorded source, adopt the current behavior
as a new baseline:

```
let baseline := [unit adoptLive]
```

`adoptLive` does not change dispatch. It makes the drifted unit historical and
answers a fresh active `#adHocBaseline` generation that later candidates may safely
supersede. It rejects raw or native drift that cannot be replayed from the portable
method-operation log.

## Images and process boundaries

Active candidate behavior is part of image code and the portable method-operation
history. Both normal mapped boot and `--rebuild` reproduce the behavior that was
current when the image was saved, including removals and rollbacks.

Native rollback pointers, candidate roots, and dylib handles are process-local and
do not cross an image boundary. On boot, saved origin generations become
`#historical` and point to a new active `#composed` generation that owns the
reconstructed dispatch ledger. Consequently, an old unit object restored from an
image is useful provenance, but it cannot roll back code from the prior process.

Candidate bundles are trusted native input, not a security sandbox. `open:` performs
byte-level inspection without loading code, and `map` makes the native-code boundary
explicit, but checksums establish identity rather than authorship.

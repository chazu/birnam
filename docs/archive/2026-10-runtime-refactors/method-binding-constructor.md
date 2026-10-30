# Shared Method binding constructor

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

Historical report. The forwarding implementation and probe described here were
retired by the [minimal stackless machinery simplification](minimal-id-stackless-machinery.md).
Current definitions bind compiled bodies directly; the original measurements below
are retained as historical evidence, not current runtime qualification.

2026-10-15. The first production slice following the
[shared-invocation investigation](shared-invocation-investigation.md).

## Delivered API

```birnam
let template := [object methodFor: #message:];
let method := [template withImplementation: [:self :argument | argument]];
[method invokeOn: object arguments: #[42]]
```

`withImplementation:` is an ordinary replaceable Method message. Execution, the
owner of Method records and frame registrations, implements its initial primitive.
The receiver must own a registered resumable program with an implementation slot.
The argument may be any object; invocation still sends the current `value:`-family
message to it. This is not a Block-only body shortcut.

The result has a fresh Method identity, registration, implementation anchor and
generation lease. It retains the selected executable owner and any image mapping.
The admitted descriptor shape is copied with a fresh executable anchor and generation;
a derived entry plan is not reused from the previous binding. Code and source-map
identity remain those of the selected program. Rebinding does not copy an old inline
optimization, candidate or native argument signature. The new implementation supplies
its native signature through the existing owner protocol.

Only immutable executable selection is shared. Neither current implementation
lookup nor another owner's service entry is cached. A retained original Method keeps
its original meaning; rebinding creates another Method. No Store record, mandatory
fact, selector-specific Rust table, new foreign payload access or new Id primitive
is involved. The construction primitive does not replace the future source-authored
choice of forwarding policy.

This API does not make arbitrary native code rebindable. Native-only Methods and
registered Block bodies without an implementation slot fail at the owner boundary
with the existing native invalid-request outcome. It does not reinterpret a
synchronous wrapper's static implementation cell as a per-Method slot.

## Ownership and recovery

The new binding retains the executable owner directly, not a chain of previous
Methods and implementations. Removing the original registration does not retire a
sibling's registration. Active logical frames retain their selected Method, body
and code; completed calls release those roots. Sharing a code owner does not give
that owner references to all new bindings.

An existing TCC Unit still retains its original initialized template and that
template's implementation. A reusable factory should initialize a dedicated
neutral template, not cache the first user's Method and body. This constructor
never appends new bindings to the Unit. The old-body reclamation probe uses a
neutral template; it does not claim to remove the Unit's existing initializer
references.

Logical frames, suspension, bounded drives and recovery continue through the
existing registered-step engine. A pending-call optimization can recover to the
shared forwarding program without replaying the entered effect. The forwarding
send still performs live lookup after outer Method selection. A new binding must
be optimized for its own implementation, rather than inheriting an optimization
of the old one.

## Construction and image evidence

The executable probes now use the production constructor:

- `shared_invocation_probe.rs` exercises native and Birnam-authored shared programs,
  arities 0, 1, 2 and 16 (the authored fixture is unary), self/argument identity,
  current lookup and DNU, captured Cells, reentry/GC, native Word/Bytes conversion,
  Error identity, frame visibility, suspension and entry recovery.
- It also checks that a rebound Method does not inherit an old inline optimization,
  that the original Method/body collect while its sibling remains callable, and
  that dead bindings collect while the shared code remains available.
- `method_binding.rs` uses ordinary generated templates through source and compiled
  artifact construction, and replaces a template's constructor through a normal
  message definition with collection inside that callback.
- Ordinary source-backed forwarding bindings survive mapped and rebuilt image
  restoration after deleting the input artifact. Restored Methods can be rebound.
  The fixture keeps Method values out of durable state: they remain process-local.
  These are in-process restore checks, not parked-frame image qualification.

The existing image path reconstructs logical implementations using ordinary
forwarding code. It does not preserve the physical sharing arrangement, and these
checks do not qualify arbitrary custom forwarding policies for portable images.
The separate native-failure/image mismatch remains unresolved; this slice changes
neither language error policy nor compilation disposition.

With an authored shared template already loaded, 16 constructor calls add **zero
TCC code units**, compared with 16 units for 16 ordinary generated definitions.
This checks actual `withImplementation:` calls on one template. It excludes the
upfront template cost and is not a CPU or total-memory improvement claim. Descriptor
metadata is still copied per binding; sharing that immutable storage is a possible
later optimization, separate from fresh anchors and registration lifetimes.

## Integration boundary and next slice

**Default Reflect construction has not switched to shared templates.** The existing
compiler, artifacts and images still generate their usual adapters. This commit-sized
slice makes code sharing usable through owned objects, with no raw entry-pointer
argument required from source. It supplies the missing construction mechanism rather
than introducing a permanent native publication owner.

The next slice should make an ordinary Birnam owner select reusable templates by
arity and call this constructor. Reflect, artifact construction and image recovery
must use that same live construction protocol. Define this override seam explicitly;
per-definition NativeBackend emission will no longer be an interception point once
code is shared. Do not cache foreign service methods or weaken TCC Unit's single-
initialization contract to implement the factory.

Keep the uniform error-result change separately reviewable. The current explicitly
synchronous artifact path has different native-failure behavior, so blindly routing
it through a resumable shared template would change publication behavior. There is
no backwards-compatibility requirement to preserve two language error models; the
replacement must instead be specified and qualified consistently across source,
native boundaries and images before switching that path. Store publication migration
remains pending.

## Qualification

See the [receipt](../../measurements/method-binding-constructor-20261015.json) for exact
inputs, runtime libraries, commands and results. Lean now checks 286 theorems and
rejects 67 mutations; TLC produces all 164 expected outcomes. The new lifetime model
rejects shared registration identity, code rooted only by the template, and a code
owner retaining every binding. These are abstract contract checks, not a whole-
runtime or compiler proof.

The fresh production build, 26 Execution unit tests, constructor tests, adjacent
Execution/native-input tests, compiler, artifact, candidate retirement, generation,
materialization and publication regressions pass. Both ordinary image roundtrip
and the 20-epoch repeated-save/live-set regression pass. The remaining language
checks (scratch state, Reflect, code inspection, Cells, Module registry, LSP and
Globals) pass, subject to the repeated-run qualification below.

The 826 module assertions pass. One existing slot-retention allowance check initially
reported seven extra objects against a six-object limit. The same exact failure was
reproduced on the previous runtime (one of five repeats); the new runtime passed
five repeats unchanged. No tolerance was relaxed. The initial aggregate script
therefore did not pass uninterrupted; its remaining checks were run separately.

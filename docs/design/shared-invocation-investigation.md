# Shared method invocation: adapter necessity investigation

2026-10-15. **Investigation and executable prototypes; no production invocation
path has been replaced.** Follows the [failure-boundary experiment](publication-failure-boundary.md)
and the user's clarification that ordinary error behavior should follow message
passing, independent of execution strategy. Backwards compatibility is not a
requirement for this work.

Follow-up: the [Method binding constructor](method-binding-constructor.md) now
implements the first production slice. The receipt below remains the original
prototype evidence; the follow-up has its own receipt and qualification limits.

## Finding

Per-definition generated named-adapter code is not necessary. The logical
forwarding operation remains useful and observable:

```text
current lookup of target's selector -> selected Method
selected Method supplies its own implementation, receiver and arguments
shared step returns a tail-send action:
    implementation value: target value: argument ...
Execution performs current lookup on implementation and enters that body
```

One shared forwarding program can serve distinct Method objects and preserve
this behavior. The narrow recommendation is to share that program and its code
ownership, while retaining independent binding data and logical frames. Do not
begin by flattening every callable into an unconditionally synchronous C call.

## What the current adapters do

`lower_named_adapter` builds a one-step logical program. It reads the original
receiver, explicit arguments and retained implementation, prepends the receiver
to the arguments, and tail-sends the appropriate `value:`-family selector. The
normal generated adapter supplies a registered frame descriptor and source/recovery
metadata. The synchronous variant emits a C forwarder with a retained
`implementation_0` value. Both ultimately use Id's method ABI.

Reflect currently compiles/loads a named adapter for each definition, or loads a
separate adapter Unit from the enclosing artifact's precompiled bytes. The arity
and forwarding code can be identical across many definitions. The binding and
retained implementation are different, but that does not require different
machine code.

There are three separate obligations:

| Obligation | Keep | Simplification candidate |
| --- | --- | --- |
| Method-to-implementation invocation | Correct `self`/arguments, current `value:` lookup, arbitrary implementation objects, DNU and captures | Share the forwarding code; bind distinct implementation data |
| Execution control | Logical frames, bounded drives, suspension, selected-call lifetime and recovery | Reuse the existing registered-step machinery; do not discard its semantics |
| Native representation boundary | Word/Bytes/Object conversion, copied borrows and declared native ownership | Keep explicit carrier adaptation; it is not a separate language error model |

A direct jump to an implementation's previously selected body would skip the
second live lookup. Replacing an implementation object's `value:` method or its
lookup protocol must affect the later send, even when the outer Method was
already selected. Conversely, replacing the target's dictionary entry must not
rewrite an entered or explicitly captured Method. The prototype checks both.

## Prototypes and executable evidence

[`shared_invocation_probe.rs`](../../rust/birnam-siphon/tests/shared_invocation_probe.rs)
uses the existing `Method fromStep:descriptor:selection:owner:implementation:`
interface. It gets the canonical descriptor/selection from the current NativeBackend
and registers separate Methods against shared step code. No new runtime primitive
or Id change is introduced.

There are two step implementations: a generic native test implementation and an
[authored Birnam unary step](../../tests/fixtures/shared-invocation-step.birnam),
compiled once through the existing artifact path. The main behavioral probes run
against either. The native variant additionally exercises arities 0, 2 and 16;
the checked-in authored fixture is specifically unary.

Each binding gets a fresh ordinary owner record pointing to the shared code owner.
This preserves distinct binding-generation identities without duplicating code.
The Method separately retains its implementation. There is no global mutable
“current implementation” and the shared code owner does not accumulate user Methods.
The source step's counter is test instrumentation, not proposed production policy.

Checked cases:

- Exact receiver/argument identities and ordering; distinct methods using the same
  entry; captured Methods retaining their original definition after replacement.
- Current implementation `value:` replacement, custom lookup and DNU, with collection
  inside callbacks; nested calls and shared captured Cells.
- Error-valued results preserving identity; comparison with the current normal
  generated adapter for native failure conversion. This does not implement the
  newly agreed uniform error contract across all execution strategies.
- Explicit native Word and Bytes input/result conversion through the existing owner.
- A bounded drive stopped before forwarding exposes the implementation, receiver
  and argument in its logical frame. Updating the target binding and implementation
  lookup at that point preserves the entered Method and observes the later lookup.
- Frame views become stale after completion. Distinct binding generations remain
  distinct even though they share executable code.
- A body can park and resume. Repeated resume is rejected. Collection while stopped
  retains the entered Method, implementation and owner; completion releases them.
- An entered optimization recovers at the pending-call entry to the shared baseline;
  the recovering entry and ordinary forwarding each execute once. This is entry
  recovery coverage, not a proof of arbitrary mid-body optimization correctness.

### Negative controls and resource observation

A plain native trampoline performs the live forwarding send but keeps a native
call boundary around the body. It does **not** preserve the park/resume behavior.
The registered shared step instead returns a tail-call action; its native stack
has exited before Execution runs the body. This is why a common C ABI does not
make these two implementations equivalent.

Caching a current TCC Unit and calling `initializeWith:` with another implementation
is also not a factory: the existing owner rejects that reinitialization. Merely
removing the rejection would be unsafe—the synchronous Unit has one implementation
cell, and Unit retention could pin every binding ever created. Share code and
create distinct binding records instead.

With the shared step already loaded, constructing 16 normal generated adapters
adds 16 live TCC code units. Constructing 16 shared-step bindings adds **zero**.
The ordinary temporary units retire after their roots are removed; the shared
bindings' Method objects are likewise reclaimed while the shared program remains
available. This is a marginal code-unit observation, not a total memory reduction
percentage or CPU speedup. The shared program has its own upfront code/data cost.

## Production design and remaining work

The prototype's forwarding policy can be written in Birnam. Its registration
harness is still native: the low-level constructor takes a raw NativeEntry and
canonical descriptor/selection bytes. The source ABI bridge does not provide a
general NativeEntry argument carrier. Parsing arrays out of emitted C, as the test
harness does, is also not a proposed language API.

The next production slice should introduce an **owner-facing binding constructor**
that accepts owned callable/code objects and an implementation, creates a fresh
Method and registration, and retains the shared code owner. Exact selectors are
not selected by this investigation. An existing Method/template could provide the
owned executable/descriptor; implementation binding must not merely mutate or
capture its old registration, because each new binding needs its own implementation
anchor and lifetime/admission identity.

Expose that construction policy through ordinary replaceable messages. Do not
hide it in a permanent Rust table of selector-specific entry pointers. A shared
step is chosen code owned by the resulting Method; it must not become a cache of
another owner's service methods. Future implementation sends remain live, and
replacement of the construction policy must affect subsequent definitions.

Keep these integration boundaries explicit:

1. **Avoid recursive entry machinery.** A production step must not call helpers
   whose own activation requires that unfinished step. Capture descriptor constants
   during construction, omit the test counter, and use the existing owner protocols
   required by the step ABI. Bootstrap must install the necessary primitive/control
   substrate before the shared language-entry program. Do not freeze dependencies
   or add monkeypatch checks to hide a recursion problem.
2. **Unify ordinary error semantics.** Application error handling belongs to ordinary
   messages/results. Adapter style, compilation strategy and image restoration must
   not choose a different rule. Genuine native representation conversion and VM/ABI
   faults still require explicit boundary handling. The prototypes leave current
   runtime error conversion unchanged.
3. **Route all construction paths coherently.** Reflect, compiled artifact loading,
   Codegen materialization and image restoration must construct the same logical
   Method binding. Define the new override seam before removing incidental per-method
   emitter calls; do not claim that old compiler interception points remain invoked.
4. **Preserve image meaning, not duplicated wrappers.** Save authoritative method
   data and code/capture identities; reconstruct through the chosen owner protocol.
   Shared-code image packaging, parked-frame roundtrips and source-free recovery
   are not qualified by this prototype. The previous image failure reproduction
   remains a regression case for the uniform result contract, not a requirement to
   retain two language error conventions. No old-image compatibility work is required.
5. **Qualify bootstrap and costs.** Cover source and artifact construction, captured
   Methods, suspended recovery, declaration/lookup replacement, code retirement,
   compiler metadata and no source reparsing. Demonstrate a bounded seed replacement
   path rather than adding a permanently native publication service. Measure actual
   invocation throughput and construction/retained memory before claiming performance.

A wholesale merger of Method and Block layouts is not needed for the first slice.
Their identities and roles remain different: the Method binds invocation policy
and implementation; the Block carries executable meaning and lexical captures.
They can share entry machinery without conflating those objects.

## Formal checks and limits

Lean `SharedInvocation` checks live lookup, receiver insertion, capture identity,
independence from code address, distinct binding identity, and counterexamples to
freezing lookup or using one mutable implementation slot. TLA+ `SharedInvocation`
models entered binding retention, replacement before the later lookup, collection,
logical-frame visibility, suspension and no replay. Deliberately broken variants
produce the expected counterexamples. These are abstract contracts, not a compiler,
heap, native-code lifetime or whole-runtime proof.

The [source-bound receipt](../measurements/shared-invocation-investigation-20261015.json)
records the two prototype runs, neighboring native integration checks, formal
results, runtime provenance and exact code-unit observations. Both prototype modes
passed, alongside existing Execution method/machine, native-input and invocation
checks. The formal gate checked 281 Lean theorems, rejected 65 mutations and
obtained all 160 expected TLC outcomes. No production module,
installed release, ABI or image format was changed. Previous uncommitted publication
experiments remain separate evidence; this report supersedes their proposed next
step of preserving two adapter-dependent error behaviors.

Reproduction:

```sh
CARGO_TARGET_DIR=.cargo-target/native-message-tests \
BIRNAM_TEST_MODULE_DIRECTORY=PREFIX/lib CARGO_PROFILE_DEV_PANIC=unwind \
  mise exec -- cargo test --offline --locked -p birnam-siphon \
  --test shared_invocation_probe -- --include-ignored --nocapture
BIRNAM_AUTHORED_SHARED_STEP=1 CARGO_TARGET_DIR=.cargo-target/native-message-tests \
BIRNAM_TEST_MODULE_DIRECTORY=PREFIX/lib CARGO_PROFILE_DEV_PANIC=unwind \
  mise exec -- cargo test --offline --locked -p birnam-siphon \
  --test shared_invocation_probe -- --include-ignored --nocapture
mise exec -- python3 scripts/check-rooting-model.py
```

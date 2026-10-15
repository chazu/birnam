# Authored method construction policy

2026-10-15. Implements the next slice after the
[Method binding constructor](method-binding-constructor.md): ordinary source
Reflect definitions now share forwarding code through Birnam policy. This report records the source-only milestone and its measurements. The subsequent
[supplied template slice](supplied-method-templates.md) integrates artifact activation
and image reconstruction; its qualification and measurements are separate.

## Delivered path

After host bootstrap, normal source construction follows:

```text
Reflect defineMethod:on:as:
  existing Generation admission
  Reflect methodForArity:implementation:
    current MethodInvocation methodForArity:implementation:
      current templateForArity:
      selected template withImplementation:
  existing Store publication/notification protocol
```

The two factory messages are ordinary, replaceable sends. The native facade sends
to its actual receiver, so a delegated Reflect facade can override construction
without replacing the canonical facade. Source name resolution
can select a replacement MethodInvocation through Globals or Namespace. A template
is an owned Method containing a chosen forwarding program; each new binding gets
its own implementation, identity, frame registration and lifetime through the
previous slice's constructor. Its eventual implementation `value:` send stays live.

The source owner is [method-invocation.birnam](../../modules/method-invocation.birnam).
`compileTemplateForArity:` asks the current NativeBackend for ordinary resumable
forwarding code and the current TCC to compile it. It initializes that Unit exactly
once with **nil**, never with the first user's implementation. New bindings do not
accumulate in the Unit's retained references. No initializer restriction was relaxed.
The forwarding emitter is still the existing NativeBackend; this slice authors
construction policy, not a new self-hosted compiler or VM.

Existing selectors on the new owner:

| Message | Purpose |
| --- | --- |
| `templates` | Inspect its ordinary persistent Map of selected templates. |
| `templateForArity:` | Select an existing template, or compile and optionally cache one. |
| `compileTemplateForArity:` | Replaceable code-production policy for a cache miss. |
| `methodForArity:implementation:` | Create a distinct Method through the selected template's current constructor. |
| `clearTemplates` | Release the owner's cached choices; installed/captured Methods remain valid. |
| `install` | Explicitly install the source Reflect construction hook. |

`Reflect methodForArity:implementation:` constructs a value without installing a
dictionary entry. Generation's existing restrictions still apply to
`defineMethod:on:as:`. The new constructor does not bypass publication policy.

## Private state and images

The template Map lives in one ordinary ObjectModel slot on MethodInvocation.
There is no new Rust cache table and no Store description. Construction and clearing
are checked to leave **Globals bindingBasis, Store basis and Id bindingEpoch unchanged**.
Publishing the resulting Method remains a separate, observable operation.

A transient Globals binding was considered and rejected: updating it still advances
the binding revision, which Generation uses to reject stale preparations. Derived
compiler state must not create that unrelated source-binding mutation. Owning the
Map directly also removes those global writes and lookups from cache management.

MethodInvocation is a named Namespace export, like other process-local owners.
The existing image codec records an export reference rather than serializing its
payload. A fresh process creates an empty cache in native bootstrap and resolves
that owner by name when loading an image. Saving does not clear the live cache.
A same-process restore may retain this process's already selected templates;
`clearTemplates` explicitly discards them. This is bounded, derived code ownership,
not durable application state or an implicit compiler-method invalidation scheme.
Explicitly retaining a template Method in durable application data remains subject
to the existing process-local Method restriction.

The image tests qualify ordinary forwarding, fresh mapped/rebuilt processes, live
factory replacement and source-free loading. They do not introduce a portable
recipe for arbitrary custom native forwarding programs or parked frames.

## Bootstrap and malleability

The factory artifact loads after Globals, but initially remains dormant. A small
native implementation of `Reflect methodForArity:implementation:` is the construction
seed. Host source bootstrap installs the authored hook only after stdlib, stackless
control, compiled-unit policy and isolate policy are in place. Prepared startup
images preserve that installed hook.

This ordering matters: initial control installation replaces Boolean's true and
false methods in separate steps. Calling a factory that itself uses those sends
in the middle would consult a temporarily incomplete protocol. The solution is an
explicit bootstrap dependency order, not frozen Boolean entries, method identity
guards, or automatic fallback when a user replaces a method. The seed has a concrete
replacement point and is not the continuing runtime policy owner.

On a miss, the factory reads the current cache after compilation, because compilation
may reenter. Publishing the optional entry compares the owned Map snapshot identity
with the current slot. If another update wins after that read, the factory keeps the
newer cache and uses its already-created template for this call without caching it.
It never retries compilation. This is an ordinary data update, not a monkeypatch
check. A later call can compile another template if needed.

Replacing the factory, its template selection method or the template constructor
changes subsequent construction. Replacing NativeBackend affects future template
compilation; existing selected code remains owned until cleared, just as a captured
Method keeps its selected code. Existing bindings continue to use current
implementation lookup. No foreign service Method is cached by the source policy.

## Result behavior

The normal source factory returns a Method or an ordinary Error. Errors propagate
through the standard Error message protocol where no state is being published;
explicit checks remain before cache writes or the next distinct service operation.
A failed construction does not publish a target dictionary entry or replay callbacks.

This is an intentional refinement at the new source construction boundary: a backend
Error is now returned unchanged, whereas the previous native source-definition path
collapsed it to a generic native invalid-request failure. Native failures encountered
by ordinary source policy use the existing language failure conversion. The factory
uses resumable language code both initially and after image restoration.

This does **not** unify Execution's broader error conventions. Explicitly synchronous
artifacts retain their existing construction path, including their current native
failure behavior. The previously characterized synchronous-failure/image mismatch
remains separate work; no adapter-disposition or image-format change is hidden here.

## Measured construction costs

Seven alternating fresh-process samples per runtime compare against `c4f1ea91`.
The production-host test uses installed startup images, the full control library,
and ordinary source definitions. The native probe isolates the same API after
explicitly installing the source policy on its smaller bootstrap world.

| Measurement, median | Before | After |
| --- | ---: | ---: |
| Installed host: 16 retained source definitions | 57.111 ms | 20.986 ms |
| Native probe: 16 retained Reflect definitions | 40.270 ms | 5.212 ms |
| Native probe: 10,000 warmed invocations | 98.874 ms | 100.254 ms |
| Installed host: 10,000-call source loop, including iteration | 2,474.860 ms | 2,471.796 ms |
| Additional TCC units for 16 unary definitions | 16 | 1 |
| Additional native code allocation for those definitions | 512 KiB | 32 KiB |

Normal host construction is about **2.72 times faster**; the narrower native probe
is about **7.73 times faster**. Invocation differences are small and go in opposite
directions in the two tests; no invocation speedup is claimed. Construction includes
the first template compilation but excludes process startup and body compilation.
The source benchmark disables collection inside its timed work and reports raw
intervals. Reproduce it with `scripts/method-invocation-bench.py`.

The owner itself adds **8 resident TCC units / 512 KiB** in the native probe's
bootstrap world. Consequently the complete measured native footprint is still
32 KiB larger after 16 unary definitions, equal at 17, and smaller from 18 onward
for this same workload. This is not a claim about RSS, the whole language heap,
or every arity. The per-binding descriptor and Method data still exist. The cost
is explicit: this slice authors policy and reduces repeated construction, but
precompiled programs do not yet benefit from sharing their supplied adapters.

## Evidence and limits

The [receipt](../measurements/method-invocation-owner-20261015.json) records source
hashes, qualified runtime provenance, commands, test outputs and measurements.
Native probes cover shared code, independent Methods, current factory/template
replacement with collection, backend/TCC Error identity, native failures, reentry
both during compilation and immediately before cache publication, and unchanged
world revisions for pure construction. Existing binding tests cover entered frames,
suspension, recovery and collection of retired bindings.

Fresh image processes check that the cache starts empty, source-backed Methods remain
callable, new construction uses the factory, saving preserves the live cache, and
running-generation mutation restrictions still hold. The host acceptance check covers
receipt integrity, relocation, source startup, diagnostics and GC. Formal checks
model selected code versus bindings, error identity, private cache revisions,
reentrant publication, no replay and exclusion of cached Methods from image data.
These remain abstract contract checks, not a compiler or whole-runtime proof.

Qualification on the final runtime covers 826 passing language assertions and all
postchecks, nine native integration binaries with precise GC, fresh mapped/rebuilt
image processes, and host startup/relocation acceptance. Lean checks 292 theorems,
rejects 71 mutations, and TLC obtains all 171 expected outcomes. The receipt records
the host acceptance rerun after disposable build prefixes were removed to recover
disk space; the initial snapshot attempt did not produce its image.

## Follow-up integration

At this milestone, precompiled artifacts carried their current adapters, and image recovery
reconstructed ordinary adapters per binding. They had not silently switched to a
new error convention or to freshly compiling source templates when precompiled code
was supplied. The [supplied-template follow-up](supplied-method-templates.md) reuses owned **resumable artifact
and image templates**, exposes a replaceable binding policy, and measures the
startup/retained-code benefit. It must preserve which code was actually selected;
arity alone is not authority to substitute a different supplied artifact.

Keep the explicitly synchronous path separate until ordinary error-result semantics
are specified and qualified consistently. Then resume the Store publication-owner
migration. No new permanent Rust publication service is introduced here.

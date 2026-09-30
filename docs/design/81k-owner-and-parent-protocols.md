# 81K — Owner and parent access through default-vtable protocols

> **Status:** Implemented and validated. Follows the production
> private-behavior extraction in [81J](81j-private-behavior-extraction.md).

## Scope and boundary

Remove `setOwner`, `vtableOwner`, `setParent` and `vtableParent` from IdApi.
Their ordinary consumers now send messages to Vtable. The native table becomes
**49 fields / 392 bytes**, declared by [IdApi v3](../../abi/idapi-v3.json).

This is a protocol refactor. The default-vtable owner and parent fields remain
in Id's existing representation. The owner association is the existing strong
GC edge; it is not executable ownership. Parent writes retain validation and
mutation-epoch invalidation. There is no new ownership tracking, code-retirement
policy, collector or allocation representation.

## Message contract

All four methods are installed during Id construction, before object-model or
other native modules load. Their receiver is the primordial Vtable object; the
candidate is an explicit argument. This preserves the old queries' ability to
inspect null, non-vtable and no-longer-registered identities without dispatching
to, or dereferencing, the candidate itself.

| Message to Vtable | Default behavior |
| --- | --- |
| `ownerOf: candidate` | Return the registered default vtable's owner, or raw null if absent/unregistered |
| `ownerOf: behavior put: owner` | Set the existing strong owner association; return behavior |
| `parentOf: candidate` | Return the registered default vtable's parent, or raw null if absent/unregistered |
| `parentOf: behavior put: parent` | Require a registered behavior and registered or null parent, write the parent, advance the mutation epoch; return behavior |

Raw null retains the existing native protocol's missing-association meaning; it
is distinct from language nil. Setters retain the previous checks. These are
operations on the default representation, not a requirement that all behavior
objects have one parent or owner field.

The four helpers in `birnam-abi` only intern a selector and send. They contain no
representation access, saved Method pointer or alternate native function table.
Replacing the installed method or lookup affects ordinary callers immediately.
As with any ordinary send, callers must preserve operands and allow allocation,
collection and reentry; these calls are no longer callback-free raw queries.

## Migrated consumers

Object-model uses the owner messages for private-behavior selection and
publication. Prelude, Collection, Store, Image, Codegen/Reflect, Portable Code,
TCC, ForeignAddress and the host migrate their ordinary owner/parent calls.
The Rust Id implementation still reads/writes its own representation directly
when implementing the default protocols, bootstrap, tracing or guarded native
operations. That does not create an additional public interface.

The Codegen cache restoration path retains its registered-vtable/non-null-parent
criterion. Each parent query runs before taking the cache mutex, and the entry
is merged immediately afterward. This allows reentry and preserves a newer entry
published by the replacement protocol. Heap membership alone is insufficient:
a reclaimed address could have been reused for an ordinary object. A regression
test exercises reentrant cache publication, absent-parent rejection and the
existing rule that a newer entry wins.

## Pure Block-construction admission

The existing warm Block optimization must not invoke user methods during
admission. Previously its producer read parent/owner through native table
entries. Simply replacing those reads with sends would introduce effects into
a path whose contract requires none.

The floor-side admission now checks, without sends or allocation, that Vtable
still has its default metacircular lookup and the exact default `ownerOf:` and
`parentOf:` methods, with no resumable replacement. It then checks the existing
bounded Block ancestry condition and reads the owner. Any failed check declines
the optimization; ordinary construction executes the current protocols.

The optional producer declaration is now
[`birnam_native_block_construction_v2`](https://github.com/chazu/birnam/blob/cffba722/abi/native-block-construction-v2.md).
Its caller supplies the validated owner fact after proving those prerequisites;
it supplies no parent/owner access function. The producer compares its current
API context, inspects the carrier and CodeObject, and retains the existing warm
construction checks without calling a method. Wrapped `send` or `intern`, or a
different Vtable identity, also withdraw admission.

The compiler-facing floor export keeps its existing signature and performs
these checks internally. The ordinary constructor uses the message adapters.
The old producer declaration is historical; it is not used as a fallback.

## ABI, loading and acceptance

Descriptor format remains v1; IdApi schema is v3, with an independent C header,
source client and manifest fingerprint. V1 and v2 fixtures remain historical.
Consumers must rebuild together. No compatibility fields remain, and no new
dylib or load-order change is needed.

The historical recovery checker froze the entire Image implementation, including
its native accessor calls. It now allows exactly the six complete owner/parent
call substitutions in this refactor, with fixed counts and unchanged operands,
before applying the existing historical comparison. All other Image code and the
v8 reader remain checked. Negative tests reject altered operands, partial
migration and unrelated changes. Receipts hash the full current source and name
this exception explicitly; they do not claim that the whole file is unchanged.

Focused coverage includes:

- All four method replacements are observed by native adapters; object-model
  observes owner getter and setter replacements during private-behavior creation.
- Default queries retain null and unregistered-candidate behavior.
- Reparenting invalidates inherited lookup immediately, including restoration.
- With native stack scanning excluded, a rooted behavior keeps its owner and
  parent alive; replacing those edges permits their reclamation.
- Warm Block admission declines replaced owner/parent/lookup methods without
  executing the owner/parent replacements; ordinary construction executes them.
- Current C table layout, copied/wrapped API context, image recovery, native
  startup and installed module loading are checked by the existing native gate.

The local exhaustive run uses the existing four CI matrix groups, with a
3600-second limit per step and unchanged commands/cases. The native gate runs
alongside the matrices using its completed installation prerequisite. The final
coverage oracle and source/artifact/prerequisite audit remain required. An
earlier matrix attempt was stopped to correct cache restoration; its partial
results are not acceptance evidence.

The source-bound receipt is `.cargo-target/validation-81k/run.json`; individual
logs and matrix receipts are under the same directory. The separate relocated
installation check writes `.cargo-target/81k-install-smoke.log`. These are local
validation artifacts. No performance qualification is claimed.

Validation passed on the final sources:

- **610 workspace tests passed**, with 28 ignored and none failed.
- **All 82 exhaustive-plan steps passed**, including the complete matrix
  coverage oracle and the final source/artifact/prerequisite audit.
- The separate relocated-install smoke passed, covering compiled modules,
  tools, projects, images, isolates and diagnostics.

These results establish refactor correctness within the existing acceptance
suite. They do not qualify a performance improvement or complete the thin-siphon
bootstrap work.

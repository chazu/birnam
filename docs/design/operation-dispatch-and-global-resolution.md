# Operation dispatch and global resolution

The investigated baseline has 195 sends inside Integer addition. Five candidates
were considered: operation dispatch, error/name lookup, guard checks, remaining
result protection and temporary Word boxing. This slice implements the first two;
the remaining three are tracked below with their proof obligations.

## Shared operation receivers

Integer's entry methods know the operation before calling the shared validation
pipeline. They now pass a shared operation Object rather than a Symbol to be
looked up in private Integer and Float dictionaries. Each operation has live
`applyInteger:with:` and, where supported, `applyFloat:with:` methods. They call
current Integer arithmetic helpers or the actual Float operators through Id.
Remainder has no Float handler; the original Float probe/rejection order remains.
Each operation's `arithmeticKind` answers its stable Symbol. Remainder tests use
that kind rather than comparing to a newly initialized object. Reload may leave
public operators pointing at an earlier operation receiver; its meaning remains
valid against the new shared validation helpers.

The five objects and their methods are created once at module initialization.
No handler or private vtable is allocated per addition. The shared error/Integer/
Float selection pipeline and noncanonical Boolean outcomes remain. Source policy,
left validation, exact Error precedence, right validation and factory calls retain
their order. `integerBinary:operation:` remains an Id method, but its private
operation argument is now an operation receiver, not an arbitrary Symbol. The
old private dictionaries/capture-inspection protocol are removed. No compatibility
shim for that private interface is added. Public arithmetic installation still
honors `hasBootstrapArithmetic:` and does not overwrite prior user replacements.

## Global resolution owned by Namespace

The error-check trace showed only three sends for `Namespace valueAt: #ErrorVt`.
Much of its cost was the compiler's late-bound resolution of global `Reflect`,
including repeated temporary registrations. Capturing `Reflect` would eliminate
more work but would change the existing Globals/Namespace rebinding behavior.
That shortcut was rejected.

Namespace provides `resolveGlobal:`. It performs the same current Id sends:
fetch Namespace `nil`, fetch Namespace `Globals`, read that Globals receiver's
`at:` for the requested Symbol, and fall back to Namespace `at:` only when the
result is the previously fetched nil. Native Unit from the final export lookup
maps to that nil. The getters are not bypassed, cached, or assumed canonical.
Their failures and malformed result handling are retained.

A nested owner scope roots fetched nil and Globals before further callbacks.
Namespace's registered `rootsDo:` reports those temporary references. Rebinding
both exports during a getter cannot destroy its earlier snapshots. Scopes drain
on success or failure. The compiler now delegates resolution through one ordinary
Id message rather than implementing that pipeline and private token bookkeeping
in every C body. Static Symbol lifetime still uses the Unit's existing inventory.
This consolidates lookup; it does not pin the current Reflect/Globals/export value.

`Reflect vtableOf:` also stops fetching an unused nil export before its actual
behavior query. The retained behavior query remains live. Its unused preparation
lookup history is not preserved as a public behavior guarantee.

No foreign Store/Globals payload is inspected, and no new Id floor function or
ABI struct is added. Existing compiled machine code keeps its embedded helper;
startup does not recompile it. Newly emitted global reads use `resolveGlobal:`.
Rust required for cold bootstrap is retained.

## Remaining candidate investigation

| Candidate | Finding and next bounded work |
| --- | --- |
| Word guards | The 31-send subtree includes fresh allocation/type work as well as checks. Stable lookup/entry comparisons could use Id's existing batched binding facts. Dynamic allocation permission and post-callback fences must remain live. Epoch equality alone does not prove unchanged permission. Next experiment should isolate stable-check batching with current-context and changed-policy tests. No guard was removed in this slice. |
| Remaining result protection | Trivial forwarding handoffs have already been removed. Remaining locals cross actual getter, helper or cleanup callbacks. The Map rehash counterexample requires its detached-node snapshot. A common owner-managed call scope or explicit result handoff may simplify tokens, but needs coverage until the consuming call returns and current-provider fallback. No additional root is deleted merely because it is called temporary. |
| Three temporary Word boxes | Authored methods accept Objects. The boxes transport bits through that interface and can be observed by retained getters/operators/factories. Removing them needs a guarded compiler region proving nonescape, relevant identity/GC observations and reconstruction before unknown calls. Pure numeric bounds alone are insufficient. Keep this after the source/API and guard work; no unboxing is claimed here. |

Stable guard batching is implemented in [the next slice](stable-guard-batching.md),
reducing the warmed operation from 181 to 177 sends while keeping dynamic permission
and final epoch fences live. The call-scope and
unboxing items are design/proof work rather than completed optimizations.

## Evidence

Lean checks the Integer/Float operation mappings, lack of a remainder Float
handler, and resolution value/getter order. Existing branch and invocation models
cover decision order and lifetime. `GlobalResolution.tla` checks fetched-value
coverage under rebinding, snapshot nil comparison, getter order and scope exit;
unsafe variants must produce counterexamples. These are conditional abstract
contracts, not a verified compiler or arbitrary native-provider proof.

Native tests replace an operation handler with a collecting method, exercise the
independent signed/mixed Float oracle, and verify global precedence, current getter
replacement, rebinding and collection of dropped resolver snapshots. The benchmark
now selects only Integer-targeted methods for policy variants and explicitly
installs matching public entry methods. Keeping an installed old `+` alongside a
new private protocol would measure a mixed policy and produced a fixture failure.

## Accepted measurements and checks

[Receipt](../measurements/operation-dispatch-and-global-resolution-20261009.json)
records hashed source versions, libraries, traces and validation. The fresh
accepted temporary prefix passes the saved-image startup gate. No user prefix or
startup implicit-compilation behavior changed.

| Inside successful addition | Before | After |
| --- | ---: | ---: |
| Total Id sends | 195 | 181 |
| Retains | 23 | 17 |
| Individual releases | 11 | 8 |
| `at:` calls | 8 | 6 |
| Symbol-byte queries | 7 | 5 |
| Private operation-table hashing | 1 | 0 |
| Explicit operation application | 0 | 1 |
| Allocated Objects | 4 | 4 |

Operation dispatch saves ten sends; lookup/reflection cleanup saves four. Total
reduction is 14 sends, 7.2%. The seven disjoint work categories are now ABI 33,
Word arithmetic/guards 31, captures 30, lifetime/cleanup 27, ordinary helper/policy
dispatch 26, validation/lookups 24 and Boolean identity tests 10. These sum to 181
and are counts, not CPU percentages. The benchmark including its fixture gives
196.25 versus 182.25 sends per addition. Three Words and one Integer still allocate.

Two four-second CPU samples observed 51.29 versus 39.22 microseconds per addition.
The earlier run had only 17 pairs and an inconclusive within-prefix result, so
this is not a reliable before/after speedup estimate. Source/library stability
checks passed. The count reduction is the robust measurement.

Acceptance passed 46 focused Rust/C-harness checks, eighteen native checks across
thirteen binaries, fourteen task assertions at quanta 128 and 1, 132 Lean theorem
checks, and all 84 TLC configurations including required unsafe counterexamples.
The Integer oracle includes repeated artifact loads, signed boundaries, mixed
Float arithmetic/remainder and exact Error propagation. The operation replacement
test pins the original Method/code while a collecting replacement is selected.
The namespace test drops and rebinds both fetched exports during a getter and
checks that temporary objects survive until resolution, then become collectible.
This is focused validation, not an exhaustive project test rerun.

The benchmark's optional `--integer-policy-source` reads and hashes an explicit
source version, allowing comparison without editing the working tree. Production
loading still preserves existing public arithmetic replacements. A fixture must
install matching public entry methods and private helpers when comparing source
protocol revisions.

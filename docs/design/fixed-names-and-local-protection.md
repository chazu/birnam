# Fixed helper names and protection within a call

Implemented 2026-10-08. Native owners retain fixed Symbol names once and use them
for current Id messages. Generated synchronous calls reuse lifetime protection
when a returned Object already has protection through that same call. These
remove naming and rooting overhead without moving Integer policy into Rust.

## Fixed helper names

Namespace retains its Symbol-validation/lookup/predicate names and reports them
as owner roots. Execution reuses its existing rooted name registry for fixed ABI
getters/factories, deferred `value` sends and common Block arities. Unusual arities
still intern their computed selector. Word guards use the existing rooted name
map for actual permission/epoch/binding queries. Codegen retains the small set of
names used by reflection and scope retention; other names retain their dynamic
fallback. Module construction roots new names until their owner is registered.

Only names are cached. Namespace binding values, method entries, permissions,
epoch verdicts, factory results and native output values remain current. Explicit
source Symbol literals and `intern:` operations are not replaced by this slice.
Repeated internal interning hooks are removed deliberately, as with the existing
generated selector cache; their old invocation history is not a compatibility
contract. Native calling conventions and image-loading policy do not change.

## Protection within a call

The existing compiler option for invocation coverage (bit 16, enabled by default)
also enables a 32-entry cache of references whose protection lasts through the
current generated call. `keep` first checks its covered invocation inputs, then
these hints. A fresh result still obtains a nonzero Root token through Id before
being remembered. Failure or a zero token cannot produce a positive hint.

A fully successful pure capture batch seeds hints with references covered by
the live receiver's immutable capture edges. Replaced/custom readers use ordinary
acquisition and can return fresh Objects. Hint eviction never releases a token;
the next encounter can register another root conservatively. All acquired tokens
are retained until ordinary cleanup, and the cache is local to the C call frame.
It is empty on each new call. The disabled control retains the prior independent
registration strategy. Resumable step functions retain their existing strategy.

No foreign payload is read. The compiler compares opaque Object identities;
equal numeric values in distinct boxes receive distinct protection. Actual program
Root messages and all remaining owner operations keep current Id dispatch.
Private registration multiplicity is not an observable required by this rule;
logical values, aliases, shared Cells, effects of actual program operations,
code lifetime and reachability remain required.

## Formal checks and production correspondence

`SymbolNames.lean` relates retained canonical Symbol identities to dispatch in
any current world and proves owner-root coverage. It rejects an incorrect name
agreement; retaining a Symbol does not permit reusing an old method entry.

`LocalReuse.lean` proves alias root equivalence, coverage after acquisition,
eviction safety and needed-value survival. `LocalReuse.tla` models two activations
and a one-slot cache, exercising reuse, eviction, collection and cleanup. Broken
models for unrooted hints, eviction that drops a root, stale hints after exit,
changed capture edges, early release and leaks must fail. Existing invocation,
capture, admission and recovery models remain checked alongside them.

The production regression compiles actual synchronous methods, replaces Root
retention with a forwarding counter, and uses precise collection with no fresh
Objects passed as callback arguments. It checks one acquisition for a repeatedly
returned fresh local, no extra capture-alias acquisition, forty fresh locals
surviving cache eviction, cleanup across calls and acquisition failures. Existing
tests cover changed readers/factories/permissions, carriers and code lifetimes.
These are scoped proofs and bounded models, not verification of the whole runtime
or arbitrary native providers. Return handoff and collector correctness remain
explicit premises.

## Measurements and acceptance

[Receipt](../measurements/fixed-names-and-local-protection-20261008.json) records
hashed traces, libraries, short CPU runs and focused acceptance. The final prefix
explicitly rebuilds early authored bodies with the retained Rust grammar seed.

| Per successful public addition | Before | Names only | Both |
| --- | ---: | ---: | ---: |
| Sends including amortized fixture work | 626.375 | 526.375 | 514.375 |
| Intern operations and their nested sends | 55 / 110 | 5 / 10 | 5 / 10 |
| Logical root registrations | 96.0625 | 96.0625 | 87.0625 |
| Id Objects allocated | 20 | 20 | 20 |

Name caching removes 100 sends; protection reuse removes another twelve sends
and nine logical registrations. Some omitted releases were inside unchanged
batch messages, so the nine registrations do not imply eighteen fewer sends.
Combined savings are **112 sends/addition, 17.9%**. Physical allocations and actual
capture-read counts are unchanged; tracked tokens finish at zero. The remaining
five intern operations are source Symbol literals, which retain their path.

Four-second CPU observations were 68.27 -> 64.34 us/addition and, in a repeat,
66.09 -> 61.00 us/addition, roughly 5.7% and 7.7% lower. The intermediate name-only
observation was 64.49 us/addition. These separate-version active-host samples are
directional, not a confidence-qualified paired version comparison or startup
measurement. Both controls within each default run have mask 29. All saved
census/timing inputs stayed unchanged during their runs.

Passed 33 affected Rust tests, twelve native integration binaries and fourteen
task assertions at quanta 128 and 1. The gates cover Symbol validation and current
lookup, changed permissions/readers/factories, signed arithmetic/errors/Float,
carriers, precise GC, root batches, forty-local hint eviction, failure and
zero-token acquisition, invocation/code lifetime and materialization. The task
probe preserves captured identity and shared Cells across wait/resume, collection
and cancellation/cleanup. A temporary release installation and its saved-tool-image
startup checks also passed; the user installation was not replaced.

The final formal run checks 112 Lean theorems, 52 TLC configurations and every
required mutation/counterexample rejection. Broader workspace/full image gates
are separate from this focused acceptance. Root leases for publicly observable
token coalescing and whole-runtime verification remain outside this slice.

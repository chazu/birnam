# Helper lifetime and numeric boxing

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

This slice reduces temporary protection sends around authored helper calls and
native Integer/Word construction. It builds on bulk capture handoff, whose warmed
addition baseline is 297 actual Id sends and four allocated Objects.

## Root-free final calls

Generated synchronous bodies use `return_call` for a final expression send. It
resolves the selector through the existing retained Symbol cache, then checks the
actual local token count. If that count is zero, it sends through current Id
lookup and returns the checked Object result directly. There is no compiler
cleanup callback between receiving the result and returning it to the caller.
The receiving caller still owes immediate result protection before another send.
This changes no frame, selected executable, bounded-drive or resumable protocol.

Cold selector resolution can add tokens. Nonempty calls use the original
result-retain and cleanup path. Failures and malformed/native non-Object results
retain their existing status behavior. This optimization does not prove or alter
the nonempty cleanup path, nor authorize deleting protection across callbacks.

Literal Symbols now share the compiled Unit's existing retained name inventory.
A successful cache entry is rooted by that Unit throughout the call. Literal
reads may record that coverage in the existing local hint cache and skip another
registration. First-use, failed initialization, disabled-cache and full-cache
cases retain normal protection. Cache entries contain Symbol identities; actual
method lookup remains live. This also removes repeated literal interning work.

## Numeric construction

Word and Integer keep private temporary Object references in an owner-local
vector. Their registered `rootsDo:` providers include a snapshot of these
references and report each through current Id visitor messages. A scope appends
before any validation/factory callback and truncates only its own suffix at exit,
including failure. This is the same protection pattern Execution already uses.
No foreign heap layout or GC state is inspected.

ObjectModel allocation borrows the caller's live behavior input and uses its own
reported temporary scope for the fresh allocated value before identity, behavior
and size queries. Its other explicit registry-based operations remain unchanged.
Numeric constructors then protect the returned allocation in their own scope
before their remaining validation calls. No callback occurs in either handoff gap.

The constructors still call the current allocator, getter and factory methods,
perform the same behavior/size checks, allocate distinct Objects and preserve
representation, malformed-allocation rejection and failures. Three Word boxes
and one Integer remain for addition. This simplifies boxing overhead; it does
not implement unboxing, fusion of the signed decision tree or allocation removal.

Native owner tracing must enumerate all active private roots; a replacement
provider must uphold that GC contract. Scope changes occur on the owner's existing
single execution thread. The model and implementation do not introduce concurrent
native calls. Compiler/private numeric protection no longer promises the old
public-registry token inventory or internal retain/release callback history.
Authored calls to Root methods remain ordinary Id sends with their usual effects.

## Evidence and limitations

Lean checks owner-reported root coverage, exact parent preservation on nested
exit and the empty-cleanup condition. Existing Symbol-name proofs cover Unit-owned
literal lifetime and current dispatch. TLA+ exercises precise collection, nested
scopes, failure/exit abstraction and immediate handoff, requiring broken variants
to produce counterexamples. These are conditional lifetime rules, not a verified
Rust/C compiler or arbitrary replacement-provider proof.

Native tests force collection during metadata validation and reenter allocation
while parent scopes are live. They verify failure and success cleanup, no numeric
Root registrations, live final target changes, malformed outcomes, and cold,
nonempty and full-cache fallbacks. Existing capture, invocation, language value,
allocator, generation and task checks cover adjacent behavior.

## Measurement and acceptance

[Receipt](../../measurements/helper-lifetime-and-boxing-20261009.json) records hashes,
activation traces, timing qualifications and acceptance logs. The fresh temporary
release build and saved-image startup gate passed. No user installation or
compiled-image startup policy changed.

| Per successful addition | Before | After |
| --- | ---: | ---: |
| Actual `+` sends | 297 | 242 |
| All `retain:` sends inside `+` | 59 | 36 |
| All `release:` sends inside `+` | 46 | 24 |
| ABI conversion work, including descendants | 66 | 45 |
| Other lifetime work | 78 | 60 |
| Word arithmetic/guard work | 48 | 42 |
| Allocated Objects | 4 | 4 |

The combined saving is 55 sends (18.5%): 31 from helper/literal handling and 24
from construction protection. Intermediate diagnostic prefixes still contained
some older authored dependencies and measured 272 then 248; the full rebuild
removes another six helper/literal sends. Including benchmark fixture work gives
298.375 versus 243.375 sends per addition. Capture setup stays at 30 sends.

Separate four-second CPU samples observed 45.05 versus 39.96 microseconds per
addition, about 11.3% lower. This is directional evidence from sequential runs
on a shared machine, not a controlled paired estimate. The runner's within-prefix
profiles both exercise production defaults; their ratio is not this slice's
before/after comparison. All source/library stability checks passed.

Acceptance passed 36 focused Rust/C-harness checks, 18 native checks across 13
binaries, 14 task assertions at quanta 128 and 1, 123 Lean theorem checks and all
68 TLC configurations, including required unsafe counterexamples. The checks
cover root reuse, precise GC, nested scopes, failures, value semantics, captures,
ABI conversion, allocator behavior, code/generation lifetime and task behavior.
This is focused validation; the exhaustive project matrix was not rerun.

The remaining boxes and protection across actual callbacks are retained. More
aggressive unboxing needs an explicit identity/escape and current-operation proof;
this slice does not assume that a temporary Word is semantically invisible.

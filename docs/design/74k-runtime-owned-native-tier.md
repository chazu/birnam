# 74K — Runtime-owned native tier under the existing interfaces

Status: implemented and correctness-verified, 2026-09-11. Twelve baseline/
candidate registration cases pass, including same-Frame recovery, nested entry,
active replacement, GC and native-owner eviction. Full repository verification
and the ordinary-prefix materialization fixture pass. All fifteen comparisons
in the seven-group prefix measurement are inconclusive. PC specialization has
no independently qualified speedup. 74L investigates the remaining canonical
lookup cost; 74J's longer E1 timing remains unqualified after two host-gate rejections.

## Problem

74J can implement each ordinary Machine send more cheaply without asking a
native caller to know whether optimization is eligible. 74F–74H additionally
select specialized method bodies and reuse reads inside one PC. Their fixture
can call private Rust guards; an ordinary external compiler artifact cannot
obtain that authority through the existing interfaces without observable
lookup effects (74I).

The user's constraint permits internal runtime changes but requires the public
API, ABI, full malleability and expressibility in Id semantics to remain intact.
Adding helper exports, adding registration fields, repurposing reserved fields,
or introducing special Machine messages would cross that constraint.

## Selected route

Keep creation, guard callbacks, admission and ownership of a candidate inside
the runtime implementation. Existing native artifacts continue to register one
baseline StepMethod using the unchanged envelope. A candidate is a disposable
process-local implementation derived from the canonical PortableProgram already
supplied at portable materialization. It never supplies new language meaning.

The existing portable staging call already carries program inventories, binding
inventories, selection and manifest bytes. `begin_staged_portable_resumable_unit`
retains a private projection of these inputs. `StagedUnit` itself still retains
admission identities and digests rather than the program bytes. This
route does not need to ask replaceable CodeObject accessors for additional
optimizer facts or change the staging call's native signature.

The accepted portable contract makes canonical program bytes executable
authority and native code a physical projection (60L section 1). This permits
investigating another projection of that same meaning. An arbitrary registered
native method, a legacy source-manifest entry without those bytes, or a program
whose exact baseline Frame layout cannot be reproduced remains baseline-only.
Descriptor identity or a digest alone is not enough to authorize replacement.

The first admitted subset is portable Block bodies with multiple PCs,
entry values, binds and simple literals. Global reads and nested Block creation
remain baseline-only: their generated shims depend on module-owned Globals and
materialization state which the runtime cannot silently substitute. This is an
optimization eligibility limit, not a language restriction. The public compiler
and arbitrary native methods retain their existing capabilities.

## Admission requirements

1. Reuse the canonical lowering and descriptor serializer. Reproduce the exact
   baseline descriptor bytes, PC numbering, slot coloring, operand allocation
   order, roots and edge plans. Decline programs the internal backend cannot
   reproduce; do not change their registration or baseline behavior.
2. Reuse the existing native compiler implementation without importing the
   TCC module's singleton IdApi state or adding its exports to the Id floor.
   Extract the embedded compiler from the TCC language module into a shared
   Rust component, retaining unchanged exports, linking and cold-bootstrap
   behavior.
3. Compile only from admitted immutable inputs. Compiler setup and diagnostics
   must not send extra language messages, allocate extra Birnam operands, or
   invoke replaceable accessors. Failure must retain ordinary execution.
4. Retain the exact baseline descriptor allocation and separately own candidate
   executable memory. Selection follows resume-token allocation. A selected
   entry keeps its executable owner until native return, even if a callback
   invalidates or retires it. Cache eviction must not retire the active baseline
   generation or leak candidate code through dead descriptors.
5. Reuse 74F/74G's same-Frame recovery at PC boundaries. Per-access guards remain
   necessary inside a native activation because an allocation or callback can
   change lookup, registration or Frame contents after entry selection. A GC
   invalidation signal, if used, remains private runtime state and fails closed
   on saturation.
6. Keep all generated operations expressible as their original Id sends.
   Changing lookup, methods, inheritance or receiver vtables must preserve the
   original send's effects. Active old Frames retain old bodies; later sends
   see current bindings. No compiler cache becomes dispatch authority.

## Required evidence

Before normal admission, exercise a real portable materialization through the
unchanged native envelope, prove candidate entry independently of answers, and
repeat the existing mutation, precise-GC, same-Frame and native-retirement
matrix. Include failed compilation, incompatible descriptors, unsupported
programs, nested entry, candidate eviction on the native stack, and baseline
generation replacement while an old Frame remains active.

Compare ordinary release prefixes with instrumentation absent, measure both
steady execution and compilation/retention cost, and retain the existing host
gates and spread criterion. An implementation that merely passes the fixture
does not establish a usable production tier or a performance benefit.

## Prefix measurement protocol

Before the first comparison run, fix seven groups of three normal release
prefixes: `5e242e70`, the preserved 74J build, and the combined 74J/74K build.
Rotate and reverse their execution order. Run the same portable materialization
fixture at 1,024 and 4,096 iterations, with one cold call, 32 warmup calls and
five repetitions per process. Record materialization, first-call and warmup
cost separately. After the canonical phase, change the program's called method
and install the real native forwarding lookup from 74J; measure five fallback
repetitions and verify changed answers and post-eviction/GC behavior.

Include the unchanged ordinary-send control at 100,000 iterations. Use the
existing host preflight and interval gates. For each pair of variants and each
warm/fallback/control row, require the median difference to exceed both observed
sample ranges before classifying improvement or regression. Any such regression
prevents promotion. Retain all samples and rejected attempts. This new portable
fixture does not replace or retroactively qualify 74J's longer E1 protocol.

The runner verifies candidate source/prefix identity and compares historical
baseline/74J floor binaries with the preserved receipts. All three floor export
inventories and the frozen API, registration, descriptor and image authorities
must agree. No instrumentation is compiled into timed prefixes.

## Implemented admission and recovery

`runtime/native_tier.rs` derives candidates from the existing portable staging
inputs. It uses the canonical lowerer, slot coloring, descriptor serializer and
per-PC emitter. Registration admits a candidate only when the reproduced
descriptor bytes and PC count exactly match the accepted baseline. The existing
native registration envelope and all its validation remain unchanged.

The private registry holds weak descriptor identities, bounded to 256 records.
Eligible bodies have 2–4,096 PCs and at most 256 KiB of generated C. Native
compilation waits for 64 StepMethod entries. The shared
`birnam-native-compiler` crate owns a relocated libtcc unit independently of the
TCC language module. Its private functions add no floor exports or IdApi state.
Failed compilation and ineligible inputs retain baseline execution.

The driver allocates the original resume token before selection. Candidate
entries omit token decoding and the PC switch; all generated body sends, operand
boxes and allocation order remain unchanged. There is no repeated-read cache.
The exact descriptor allocation, active Frame and baseline generation stay
authoritative. Mutation, guard failure or eviction withdraws the candidate and
the next PC resumes through that Frame's original baseline. Each native entry
retains its executable owner until return, including during GC, nested entry
and cache eviction.

Twelve paired cases pass in debug and release: canonical execution, method
mutation, custom lookup, private Machine receiver, precise GC, eviction while
native code is active, saturated epochs, active body replacement, nested entry,
failed compilation, incompatible descriptor bytes and aborted publication.
Independent native-entry and baseline-decoding observations prove selection
and recovery. Old active Frames keep their bodies; a later send sees replacement.
The ordinary installed-prefix fixture also verifies portable materialization,
CodeObject identity, changed answers, materialization eviction and GC.

The [verification receipt](../measurements/74k-verification.json) records the
full workspace, native-prefix, cold-archive and relocation gates. The floor's
25 exports and the TCC module's exports, install name and dependencies agree
with the prior contract.

## Measurement result

The [retained measurement](../measurements/74k-runtime-owned-native-tier.json)
contains all seven host-gated groups, source and prefix hashes, raw outputs and
setup costs. All fifteen warm, fallback and control comparisons are within the
observed ranges. The following medians are microseconds per timed repetition:

| Work | Original baseline | 74J | 74J + 74K |
| --- | ---: | ---: | ---: |
| Warm, 1,024 iterations | 41,526 | 35,692 | 35,984 |
| Fallback, 1,024 iterations | 44,794 | 45,758 | 44,848 |
| Warm, 4,096 iterations | 217,929 | 188,269 | 187,917 |
| Fallback, 4,096 iterations | 219,816 | 220,139 | 227,930 |
| Ordinary-send control | 4,702,572 | 4,233,125 | 4,212,310 |

These medians do not establish improvement or regression under the declared
criterion. Embedding the compiler increases the floor from 731,952 bytes in
74J to 1,334,576 bytes, a 602,624-byte cost. This revision establishes native-tier
admission and same-Frame recovery under the existing interfaces; its timing
does not justify a PC-specialization performance claim. 74H read caching remains
an experiment. Further dispatch optimization is measured separately in
[74L](74l-guarded-default-lookup.md).

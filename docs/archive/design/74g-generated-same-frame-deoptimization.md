# 74G — Compiler-generated same-frame recovery

Status: experiment completed 2026-09-10; debug, release and full repository
verification pass. Authorized after committing 74F as `43935092`.
The protocol below was recorded before implementation and execution.

## Problem and proposed solution

74F switched between hand-written, statically linked entries. This experiment
starts with one canonical CodeProgram, uses the production portable lowering,
slot coloring, descriptor-v2 construction and C emitter, compiles native code,
and proves recovery and executable lifetime through the real Runtime driver.

The first optimization is **program-counter specialization**. A candidate entry
contains exactly one existing basic block and retains the ordinary argument-count
check. Runtime selection already knows the committed PC, so that entry omits the
resume-token decode and switch. Its operations, sends, allocation order, action
publication and shared descriptor remain unchanged. No assumption is carried
across a callback inside the body. The driver allocates the normal resume token,
checks the exact descriptor, PC and frozen dependency epoch, and pins the selected
native unit before entering it. Invalidation selects the original whole-method
entry, with the same Frame and PC.

This small optimization deliberately isolates the new compiler/runtime contract.
It is not the 74D boxed-index optimization. New slot helpers, changed IdApi fields,
image versions, descriptor formats and persisted optimization state are excluded.
The compiler entry point is development-feature-only; Runtime selection and the
native loader adapter are test-only. Ordinary emitted C must remain byte-identical.

## Input and admission

The fixture is a canonical CodeProgram built using the existing pure compiler
AST-to-portable encoder. Its Birnam meaning is:

```birnam
[:input |
  let carried := [input probeDouble]
  let alias := carried
  [Probe probeEffect: carried alias: alias]
  [Probe probeFinish: carried alias: alias]]
```

The source parser is not under test; the canonical program bytes are the actual
input. Both versions lower those same bytes and use one physical layout. Their
canonical descriptor bytes must agree exactly. The admission record retains
the program digest, descriptor bytes, exact baseline descriptor allocation and
dense PC-to-candidate-entry mapping. A mismatched descriptor or absent/out-of-range
PC must not authorize a candidate. The program's actual colored slots and edge
copy/clear plans, rather than 74F's hand-authored uniform bitmap, govern recovery.

## Native lifetime

Baseline and candidate are separate temporary native libraries. A fixture-local
adapter supplies the existing generated-code shim signatures through function
pointers; it creates no runtime exports and changes no frozen API. The adapter
is not a production artifact-admission mechanism.

The baseline owner holds an existing registration-generation lease token. The
Runtime keeps that token while bindings, acquisitions, Frames or continuations
can still enter the generation. Withdrawing a binding is not permission to
unmap its code. The test owner may release the library only after retirement
and the token's other holders have gone. Candidate admission owns its library;
each selected entry also holds an independent native-code lease through return.
Withdrawing the candidate during a native shim call must not unmap the currently
executing entry. `dlclose` occurs after the last required owner releases it.

The experiment must verify actual unload calls and subsequent loader absence,
not only clear a Rust map or decrement a diagnostic count. Dynamic code is never
entered through an unowned pointer, and no Runtime borrow crosses a native call.

## Frozen checks

- Execute baseline and candidate from the same CodeProgram; require identical
  answers, once-only effect order and alias identity.
- Inspect emitted C to prove candidate entries omit PC decoding/switches while
  baseline emission remains byte-identical to the committed compiler.
- Force epoch invalidation after the effect, including method replacement and
  custom lookup; later sends see current behavior, while the active caller
  resumes its original body at the compiler-assigned continuation PC.
- Force precise GC and validate compiler-derived live slots through recovery.
- Reject mismatched descriptor bytes and invalid/missing candidate PC mappings.
- Withdraw candidate admission while its native entry is on stack; prove the
  current call remains mapped and the unit unloads only after return.
- Withdraw the baseline binding while its Frame is active; prove the generation
  remains mapped until the Frame and registration lease release it.
- Repeat load/execute/recover/retire cycles and check Frame, acquisition, native
  lease and loader cleanup. Run both debug and release Rust profiles.
- Preserve runtime ABI and image/descriptor authorities and golden fixtures;
  run ordinary compiler tests and the full repository verification gate.

## Measurement and limits

Record generated source sizes, native library sizes, compilation cost and warm
execution samples separately from correctness. Timings are diagnostic unless
the existing host gates permit a controlled comparison. Candidate activation
must be proven independently of elapsed time. No performance percentage is a
continuation threshold.

This remains a bounded experiment, not an installed optimizer. Source-parser
integration, production artifact admission, persistent optimized tiers, inlining,
eliminated objects and mid-native-activation recovery remain separate work.

## Implementation and observations — 2026-09-10

The generated-code experiment passes in debug and release. Reproduce it with:

```sh
mise run generated-deopt-experiment
python3 scripts/generated-frame-deopt.py --output docs/measurements/74g-generated-frame-deopt.json
```

The [receipt](../../measurements/74g-generated-frame-deopt.json) includes source
hashes, compiler versions, native compile flags, compatibility hashes, all
observations and separate diagnostic costs. It compares compatibility
authorities against the fixed 74F commit `43935092`, including the existing
runtime API, image code, descriptor admission and registration implementation.

The input is 970 canonical bytes. Production lowering assigns three PCs and
nine slots. `carried` and `alias` occupy slots 0 and 1; the root bitmap words
are `[103, 511, 503]`. The canonical descriptor occupies 1,840 bytes and is
identical for independently lowered baseline and candidate programs. This
fixture exercises call-result initialization and dead-slot clearing; its
colored edges need no location-to-location copies.

The baseline C golden was captured from the unmodified `43935092` compiler
before adding PC specialization. The fixture checks complete baseline equality
and compares each candidate's body with the corresponding original case body.
Only the PC decode/switch is removed. The `birnam-codegen` development feature
exposes the specialized emitter; `birnam-id` uses it through a dev-dependency.
Runtime selection and the fixture adapter compile only into macOS tests.
The epoch guard is conservative: this initial optimization depends on the
committed PC and layout, not frozen method dictionaries. Dictionary mutations
exercise the recovery transport for future optimizations with such dependencies.

`Fixture` retains the program digest and canonical descriptor bytes throughout
admission and execution. `Trial` retains the exact live baseline descriptor,
generation lease and candidate entry map. A missing map entry falls back at
that PC; malformed or duplicate PC keys reject admission. The maps and native
pointers come from the test's own compiler outputs. This does not validate
arbitrary third-party native code or implement production artifact admission.

Each matrix row runs both forms. Every answer is 28, with exactly `double`,
`effect`, `finish` in order, alias identity preserved, and the same generational
Frame handle at PCs 0, 1 and 2. The replacement case additionally answers 999
on the next ordinary send.

| Candidate condition | Candidate / baseline entries | Recoveries |
| --- | --- | --- |
| Canonical | 3 / 0 | 0 |
| Epoch mutation restored, active binding replacement, custom lookup, precise GC | 2 / 1 | 1 each |
| Descriptor byte mismatch, equal-identity/different-allocation descriptor, invalid PC, duplicate PC | 0 / 3 | 0 each |
| Missing recovery PC | 2 / 1 | 1 |
| Candidate withdrawn inside its own native shim call | 1 / 2 | 1 |
| Baseline binding withdrawn while its Frame is active | 2 / 1 | 1 |

Precise GC runs inside the effect with an empty native root range. An unrooted
control Integer is collected while both compiler-carried aliases survive. The
next entry checks the values after the driver commits the colored edge.

Executable ownership is exercised with two separately compiled native libraries.
Each selected entry holds an `Rc<NativeUnit>` through native return. Withdrawing
candidate admission inside `birnam_intern` leaves its library mapped until the
entry returns. Baseline retirement is refused while the existing registration
generation token has other holders, including the active Frame's registration
pin. After return and binding retirement, the last owner calls `dlclose` and
`RTLD_NOLOAD` confirms loader absence. No frozen runtime structure gains a
loader handle or destructor.

The twelve paired cases, sixteen additional load/run/retire cycles and two warm
samples produce **84 confirmed unloads per Rust profile**. Completed runs have
no live Frames, acquisitions, continuation pins, generation leases or live
registrations. On a failed assertion the test guard deliberately retains any
still-registered native owner rather than unmapping reachable code; such a
failed run cannot produce a passing receipt.

Native C uses `cc -dynamiclib -O2`. Including the adapter, baseline/candidate
sources are 6,252/6,334 bytes and native files are 33,632/33,680 bytes. The
candidate omits all 384 PC decodes in the 128-call warm sample. Each sample
follows sixteen warmup calls; timing includes test instrumentation. Compilation
and warm times are recorded as single diagnostic samples, without host gates
or a speedup claim. This establishes the compiler/recovery/lifetime contract;
it does not establish a worthwhile production optimization.

Validation: the 61 ordinary compiler tests pass; 74F's eleven records remain
identical in debug and release; 74G's thirteen correctness records and compiler
shape agree across profiles. `mise run verify` passes the workspace,
native-prefix, cold-archive and relocated-runtime gates. Two independently
built source archives produce identical source/output closures. An earlier
verification attempt correctly rejected a source edit made during its build;
the final run held the source tree fixed and passed. The ABI fixtures and
recorded compatibility authorities remain unchanged.

## Next decision

The transport proof supports an ABI-preserving path forward. The next useful
experiment is a meaningful optimization inside one committed PC, with guards
placed after any allocating or reentrant operation and the same executable
ownership rules. Select that operation and its semantic matrix before adding
it. Production tier admission and an end-to-end parsed source journey remain
separate work; this test-local adapter is not an installable optimizer.

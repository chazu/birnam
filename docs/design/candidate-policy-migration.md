# Candidate policy migration

2026-10-18. Sequence: simplify registry validation; choose the candidate failure
contract; migrate that policy to Birnam. This continues the source-registry port.

## 1. Registry validation

The registry used `Codegen sourceRevisionOf:` only to read the declared module
name. That operation prepared/decoded retained metadata, decoded it again to
construct the revision, and constructed a larger revision value. Validation now
uses the existing `moduleMetadataOf:` query. It still requires a prepared record,
checks retained metadata/source consistency, and rejects a mismatched name.
No cache, new primitive or method-identity guard is introduced.

An instrumented Host regression measures exactly one retained-metadata read per
validation (the old build fails with two). Replacing `sourceRevisionOf:` no longer
participates; replacing `moduleMetadataOf:` still affects validation and its Error
is preserved by identity. The fresh-prefix build, parser/compiler-disabled startup,
21 source-registry assertions and native source inspection/loading suites pass.
The [receipt](../measurements/registry-validation-20261018.json) records the evidence.
Logs and the independent before/after prefixes are under
`/tmp/birnam-candidate-policy/`; qualification uses `metadata-prefix`.

Three interleaved samples with fresh processes, warm filesystem caches and precise
GC measured startup plus a trivial script at median 4.477s before and 4.342s after.
A workload including startup and 30 complete registry validations measured 8.968s
before and 6.721s after (about 25% faster). The call counts establish eliminated
work; the small timing sample does not establish a general language speedup or
prove that all of the earlier startup regression has disappeared.

## 2. Candidate failure contract

Ordinary message dispatch, Id-only module interoperability, deoptimization
and independently owned executable lifetimes remain language invariants. Automatic
compensation after candidate failure is a separate feature choice, not a condition
of native memory safety or of explicit conditional undo.

The selected contract retains conditional preflight and explicit undo, and removes
**automatic compensation on activation or undo failure**. Successful coordinated
edits remain useful; automatic repair is not needed to retain executable code or
make the undo conditional. Its reservation banks, inverse-on-failed-inverse paths,
and repeated global/lifecycle postconditions were substantially larger than that
requirement. They also could not promise arbitrary callback atomicity.

- Preparation observes the selected source, targets, Methods and Globals without
  executing the candidate factory. A prepared request is consumed once.
- Activation constructs its supplied factory once, checks the selected values,
  records each intended write before calling its current setter, and stops on
  Error. It does not replay setters or run an automatic inverse.
- A failed application may have changed code/data. Its attempted writes remain
  available for explicitly requested undo while the carrier is retained.
- Undo uses the recorded owner and before/after values. Already-restored values
  are skipped; a differing later write is preserved and reported as conflict.
  Undo itself can fail after partial progress and can be requested again.
- Description fields are history, not transaction authorization. No-op or replaced
  setters are not policed by postchecks. Ordinary Error values propagate; native
  failures follow ordinary authored-message result conversion.
- Constructor failures do not authorize replay. A caller can discard/reopen a
  carrier deliberately. Ordinary method edits acquire none of this policy.

This is an explicit change to the candidate API's failure contract, aligned with
the selected responsibility boundary for image application. It is not a change to
Id dispatch, GC/code ownership, calling conventions or deoptimization.
This removes the candidate's use of Globals compensation reservations; the
generic optional Globals reservation operations themselves are unchanged.

## 3. Authored policy boundary

The port puts preparation, construction sequencing, activation, explicit
undo, active-unit queries and discard policy in `candidate-policy.birnam`.
Native Generation retains artifact admission/mapping, optional certification and
an opaque ordinary policy Cell rooted by each existing process-local artifact
carrier. Native code never interprets the Cell's contents. Discard releases the carrier before updating descriptive fields; an
Error from a replaced description setter cannot resurrect the handle. The Cell and its data
are inspectable through the carrier's Id protocol; there is no second native
policy representation or new registry service. This bridge is tied to the remaining
artifact implementation, not a permanent bootstrap dependency.

Durable CompiledUnit records and preparation descriptors do not point back to that
process-local state. Image application releases carriers and their policy roots;
restored descriptions therefore cannot recreate authority. Explicitly retaining a
policy Cell also retains its code/resource references and need not be imageable.
Execution/TCC remain the physical code owners.

A small generic Execution binding-observation surface returns the actual installed
Method, or a captured foreign native binding that retains its code. Exact binding
comparison and restoration use Id's existing dictionary messages. Foreign native
bindings must retain their original raw ABI on restoration; ordinary methodFor:
capture behavior is unchanged. This is owner-level representation conversion, not
Generation inspecting foreign layouts or guarding ordinary sends against replacement.

The native preparation and transaction files are removed (1,241 lines); a small
read-only observation helper remains for optional certification. Candidate policy
is now 418 lines of Birnam. The existing carrier owns one opaque ordinary
Cell, not a second native representation of plans or undo journals. The generic
Execution additions expose direct Method observations and exact binding matching
through Id; they also preserve the raw ABI of foreign native bindings on restore.
A direct-native observation reuses its existing entry with a Boolean flag; it does
not add a second native-entry copy to every Method.

The policy participates in the five-module source bootstrap and is retained in
compiled tool metadata. There is no new native policy seed. Task cancellation uses
the existing TaskControl cleanup protocol to release the per-carrier operation
scope while retaining attempted writes for explicit undo. Each conditional
check-and-write runs in an existing TaskExecutor transition so automatic budget
scheduling cannot split it. This does not make arbitrary replaced callbacks atomic:
they still have their ordinary effects, and explicit waits inside a transition
follow TaskControl's existing error behavior. A before/after probe observed four
writes outside a transition before the fix and zero afterward.

## Qualification and limits

Qualification results and source/log hashes are recorded in the
[receipt](../measurements/candidate-policy-20261018.json). This is a cached Cargo
development build into a fresh disposable prefix, not a cold release qualification
or user installation. The migration does not prove Store-free full bootstrap:
TaskExecutor and other pending owners still have their existing dependencies.
Artifact admission/mapping and optional certification remain native; their future
migration must remove the carrier bridge rather than preserve it as a permanent
native policy service.

The formal models establish bounded failure/undo, conflict preservation, at-most-once
construction and executable lifetime properties. They do not verify the production
compiler. Earlier PreparedAdmission/SourceOwner examples remain historical models
of stronger compensation/postcondition contracts; CandidateUndo is the model for
this slice. The native tests exercise both synchronous and resumable candidates;
Host tests exercise ordinary task failure and entered-code replacement.
Native fault-injection fixtures now distinguish direct from inherited bindings
after Generation acquires authored behavior. They locate inherited primitives
through lookup and restore the original dictionary absence. Other fixture updates
replace automatic-compensation expectations with explicit undo, root observed
selectors across precise GC, and check the expanded installed module inventory.


Qualification completed: 847 language assertions; all 17 Host generation checks
(across the group and its corrected image/CLI continuation); native candidate,
certification, invocation and image suites, including 20 repeated image-resave
cycles; installed-manifest/relocation checks; and parser/compiler-disabled startup.
Lean checked 391 theorems and rejected 122 mutations; TLC produced 266 expected
outcomes. Execution, Generation and HostEntry retain their original native exports
and dynamic dependency sets.

The complete policy matrix ran before the final representation-only Execution
compaction. The final `compact-prefix` build repeated the seven affected native
suites, four Host binding/task/source/entered-code checks, lifecycle-record image
roundtrips, parser/compiler-disabled startup and all 847 language assertions.
Production source hashes match the packaged source. The receipt distinguishes
these builds and records the fixture corrections; this is not a new release or a
user-prefix installation.

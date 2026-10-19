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

Investigation and implementation status will be recorded here before the policy
cutover. Ordinary message dispatch, Id-only module interoperability, deoptimization
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

## 3. Authored policy boundary

The planned port puts preparation, construction sequencing, activation, explicit
undo, active-unit queries and discard policy in `candidate-policy.birnam`.
Native Generation retains artifact admission/mapping, optional certification and
an opaque ordinary policy Cell rooted by each existing process-local artifact
carrier. Native code never interprets the Cell's contents. The Cell and its data
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

Implementation and qualification of this boundary are in progress.

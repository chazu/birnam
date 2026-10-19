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

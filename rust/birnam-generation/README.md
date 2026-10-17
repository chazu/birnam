# Generation

This owner holds candidate artifact/mapping capabilities, selected change records,
source registration and optional source certificates. It does not define ordinary
message dispatch. Foreign owners are accessed through current Id messages; no
foreign payload layouts or new service table are used.

`CompiledUnit` is the authored facade. Candidate envelopes (ABI 5) bind exact
source/artifact/selection bytes to the previous revision of **that module**.
Compilation requires registered source, not an adopted whole-world baseline.
Identity, dependency and residual-initializer compatibility remain checked.
Inspection and mapping do not run the candidate factory; construction runs once
and is retained across retries.

`prepareActivation:plan:` and `prepareRollback:plan:` accept `[action, unit]`.
Their one-use tokens retain selected target/behavior identities, direct Methods,
projected Globals (including presence), lifecycle fields and one source record.
Commit sends to current setters and checks actual results, preserving Errors and
other effects. Conditional compensation/undo never erases a differing callback
write. Source publication replaces only the selected module row. Independent
modules can roll back independently; shared bindings and changed selected source
records can conflict. No source-registry-wide mutation count expires a token.

Whole-program certification is separate: `DispatchGeneration adoptCurrent`
projects registered source and calls `adoptInitialGeneration:`. Re-certification
replaces only that optional ledger. `reconcileCandidate:` / `validateImageGeneration`
report current agreement without lifecycle setters or inverse revocation. A
candidate commit does not manufacture a certificate or update a global generation.

`sourceRegistry` is `[#sourceRegistryV1, [name, compilationRecord], ...]`.
Registration is not sealed. Source records and image staging are rooted across
collecting callbacks; stage exchanges retain owner revisions to reject stale
publication. Image load suspends old capabilities and releases them after success
or partial application, without automatically adopting. Snapshot admission checks
imageability at registered authored selectors, not equality to old source bodies.
Actual live code and aliases are restored independently of these descriptions.

Entered Methods, captured Blocks and suspended Machines retain code through
Execution. GC/TCC checkpoints govern physical reclamation. Logical candidate
ownership never authorizes forcibly unmapping their code.

In this slice activation still goes through Host EvolutionIntent admission, and
discard still couples root release with file cleanup/diagnostics. Those are being
removed separately. Detailed contracts/evidence:

- [module-scoped candidates](../../docs/design/module-scoped-candidates.md)
- [observational certification](../../docs/design/opt-in-generation-baselines.md)
- [image adoption](../../docs/design/explicit-image-adoption.md)
- [compilation record ownership](../../docs/design/retained-compilation-records.md)

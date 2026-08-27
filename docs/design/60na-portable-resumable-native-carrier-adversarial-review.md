# 60NA — Adversarial Review: Portable Resumable Native Carrier

> **Status:** Review of 60N against accepted Designs 60B, 60E, 60I, 60J, 60K,
> and 60L. The resolutions below are incorporated into 60N.

## 1. A program digest became lookup authority

**Attack:** A 32-byte selection digest or `u64` portable identity could be used
as the repository key without comparing complete PortableProgram bytes.

**Resolution:** Digests index candidates only. Admission and reuse compare the
complete inventory, selection, subject bytes, descriptor, compiler contract,
and ABIs. The `u64` value remains diagnostics only. **Resolved.**

## 2. SourceRevision was renamed rather than removed

**Attack:** Hashing a PortableProgram into the legacy `source_revision` field
would make the old manifest parse while preserving the exact conflation 60K
amends.

**Resolution:** Portable staging has a distinct manifest domain, begin seam,
and descriptor admission profile. No portable row contains a SourceRevision or
lexical source path. **Resolved.**

## 3. Optional provenance still changed executable identity

**Attack:** FrameDescriptor v2 historically embeds source maps, so source text
could leak back into descriptor hashes and prevent reuse.

**Resolution:** The portable profile uses the canonical unavailable-source
spelling and zero source-map identity. Optional provenance is a separately
hashed debug artifact and is outside the materialization key unless the chosen
debug policy explicitly includes it. **Resolved.**

## 4. Reusing registration v2 permitted profile confusion

**Attack:** An envelope flag could ask the Runtime to validate one descriptor
as legacy or portable, allowing an artifact to choose weaker checks.

**Resolution:** The Runtime chooses the profile solely from its already
admitted staging context. The v2 envelope has no profile flag. Legacy and
portable begin seams reject cross-format bytes. **Resolved.**

## 5. Synchronous rows disappeared from artifact agreement

**Attack:** A manifest containing only resumable entries might allow an
artifact to ignore or silently reselect synchronous rows.

**Resolution:** Staging admits the complete CodeMaterializationSelection and
its authority. The portable manifest contains exactly every resumable row and
no others; complete selection bytes still govern both dispositions. **Resolved.**

## 6. Nested CodeObjects could be published without realizations

**Attack:** Compiling only the requested outer program could later construct a
nested Block whose selected vtable was never materialized.

**Resolution:** The admitted PortableProgramInventory is the exact transitive
closure and selection contains one BlockBody row for every program. Publication
requires every selected row either to be realized in the unit or already
admitted through the repository before the outer Block is returned. **Resolved.**

## 7. A dead TCC unit could free recognized addresses

**Attack:** Repository eviction may allow a Unit finalizer to call `tcc_delete`
while a Runtime registration, Frame, Block, or dispatch reader still owns its
code.

**Resolution:** Blocks retain the unit through their private vtable carrier;
Frames and readers retain physical pins. GC removes dead vtable bindings before
unit finalizers. A portable Unit finalizer must successfully retire its Runtime
generation before freeing TCC state; failure is fatal rather than use-after-
free. **Resolved, requires executable ordering proof.**

## 8. Portable staging narrowed ordinary sends

**Attack:** A special CodeObject invocation path could bypass vtable lookup or
make only Block implementations eligible for reflection.

**Resolution:** Materialization publishes the same Method-shaped thunk into an
ordinary private vtable. `IdApi.send` and Machine calls use the existing shared
resolve-and-pin authority. Reflect remains protocol-based and CodeBinding work
remains K4. **Resolved.**

## 9. JIT and mapped v7 acquired separate conventions

**Attack:** A one-off K3 staging shortcut could pass runtime tests but fail to
compose with K5 image admission.

**Resolution:** Both paths use the same inventories, selection, portable
manifest, identity derivation, descriptor profile, registration-v2 envelope,
and Runtime staging state. Only byte transport differs. **Resolved.**

## 10. FrameDescriptor layout was needlessly versioned

**Attack:** Introducing layout v3 merely to replace source-derived identity
would contradict 60L's explicit FrameDescriptor-v2 requirement and duplicate
the already-frozen execution schema.

**Resolution:** 60N keeps layout v2 and adds a separately named portable
admission profile. Root maps, slots, edges, calls, and generated StepMethod ABI
remain unchanged. **Resolved.**

## 11. Conclusion

The carrier requires no new language or execution-policy decision. Its private
format is a source-independent projection of already-accepted 60K meaning and
60L materialization rows into the existing 60E lifetime substrate. The
remaining risks are implementation correctness, malformed-input coverage, and
quiescent retirement ordering, all represented in 60N's acceptance list.

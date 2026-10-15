-------------------------- MODULE MutationLayers --------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "running-guard", "mirror", "rewrite-entered", "stale-commit"}
VARIABLES phase, dictionary, epoch, prepared, entered, optionalCalls, attempted, batch
vars == <<phase, dictionary, epoch, prepared, entered, optionalCalls, attempted, batch>>
Init == /\ phase = "enter" /\ dictionary = 0 /\ epoch = 0 /\ prepared = 0
        /\ entered = 99 /\ optionalCalls = 0 /\ attempted = FALSE /\ batch = "pending"
Enter == /\ phase = "enter" /\ phase' = "edit" /\ entered' = dictionary /\ prepared' = epoch
         /\ UNCHANGED <<dictionary, epoch, optionalCalls, attempted, batch>>
(* An ordinary edit during an entered call needs no managed authorization. *)
Edit == /\ phase = "edit" /\ phase' = "batch" /\ attempted' = TRUE
        /\ dictionary' = IF Bug = "running-guard" THEN dictionary ELSE 1
        /\ epoch' = epoch + 1
        /\ entered' = IF Bug = "rewrite-entered" THEN 1 ELSE entered
        /\ optionalCalls' = IF Bug = "mirror" THEN 1 ELSE optionalCalls
        /\ UNCHANGED <<prepared, batch>>
Batch == /\ phase = "batch" /\ phase' = "done"
         /\ dictionary' = IF epoch = prepared \/ Bug = "stale-commit" THEN 2 ELSE dictionary
         /\ batch' = IF epoch = prepared \/ Bug = "stale-commit" THEN "committed" ELSE "stale"
         /\ UNCHANGED <<epoch, prepared, entered, optionalCalls, attempted>>
Next == Enter \/ Edit \/ Batch
Spec == Init /\ [][Next]_vars
OrdinaryEditAllowed == phase = "batch" => dictionary = 1
NoOptionalOwnerCalls == optionalCalls = 0
EnteredSelectionPreserved == phase # "enter" => entered = 0
StaleBatchDoesNotOverwrite == phase = "done" /\ attempted => dictionary = 1 /\ batch = "stale"
=============================================================================

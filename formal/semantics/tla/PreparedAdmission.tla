------------------------- MODULE PreparedAdmission -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "epoch", "binding", "target", "global", "metadata", "source", "noop", "error", "effects"}
VARIABLES phase, binding, target, global, metadata, source, other, epoch,
          admitted, matching, wrote, failed, succeeded
vars == <<phase, binding, target, global, metadata, source, other, epoch,
          admitted, matching, wrote, failed, succeeded>>
Init == /\ phase = "prepared" /\ binding = 1 /\ target = 1
        /\ global = "absent" /\ metadata = 1 /\ source = 1
        /\ other = 0 /\ epoch = 0 /\ admitted = FALSE /\ matching = FALSE
        /\ wrote = FALSE /\ failed = FALSE /\ succeeded = FALSE
ActualMatch == binding = 1 /\ target = 1 /\ global = "absent" /\ metadata = 1 /\ source = 1
Check == (binding = 1 \/ Bug = "binding") /\ (target = 1 \/ Bug = "target")
         /\ (global = "absent" \/ Bug = "global")
         /\ (metadata = 1 \/ Bug = "metadata") /\ (source = 1 \/ Bug = "source")
         /\ (Bug # "epoch" \/ epoch = 0)
ChangeBinding == /\ phase = "prepared" /\ binding = 1 /\ binding' = 2
                 /\ UNCHANGED <<phase, target, global, metadata, source, other, epoch, admitted, matching, wrote, failed, succeeded>>
ChangeTarget == /\ phase = "prepared" /\ target = 1 /\ target' = 2
                /\ UNCHANGED <<phase, binding, global, metadata, source, other, epoch, admitted, matching, wrote, failed, succeeded>>
ChangeGlobal == /\ phase = "prepared" /\ global = "absent" /\ global' = "nil"
                /\ UNCHANGED <<phase, binding, target, metadata, source, other, epoch, admitted, matching, wrote, failed, succeeded>>
ChangeMetadata == /\ phase = "prepared" /\ metadata = 1 /\ metadata' = 2
                  /\ UNCHANGED <<phase, binding, target, global, source, other, epoch, admitted, matching, wrote, failed, succeeded>>
ChangeSource == /\ phase = "prepared" /\ source = 1 /\ source' = 2
                /\ UNCHANGED <<phase, binding, target, global, metadata, other, epoch, admitted, matching, wrote, failed, succeeded>>
Unrelated == /\ phase = "prepared" /\ other = 0 /\ other' = 1 /\ epoch' = 7
             /\ UNCHANGED <<phase, binding, target, global, metadata, source, admitted, matching, wrote, failed, succeeded>>
Admit == /\ phase = "prepared" /\ matching' = ActualMatch /\ admitted' = Check
         /\ phase' = (IF Check THEN "setter" ELSE "rejected")
         /\ UNCHANGED <<binding, target, global, metadata, source, other, epoch, wrote, failed, succeeded>>
Setter(write, error) == /\ phase = "setter" /\ wrote' = write /\ failed' = error
                       /\ binding' = (IF write THEN 3 ELSE binding)
                       /\ other' = 2 /\ epoch' = epoch + 2 /\ phase' = "verify"
                       /\ UNCHANGED <<target, global, metadata, source, admitted, matching, succeeded>>
Verify == /\ phase = "verify" /\ phase' = "done"
          /\ succeeded' = ((binding = 3 \/ Bug = "noop") /\ (~failed \/ Bug = "error"))
          /\ other' = (IF Bug = "effects" THEN 0 ELSE other)
          /\ UNCHANGED <<binding, target, global, metadata, source, epoch, admitted, matching, wrote, failed>>
Next == ChangeBinding \/ ChangeTarget \/ ChangeGlobal \/ ChangeMetadata \/ ChangeSource
        \/ Unrelated \/ Admit \/ (\E write, error \in BOOLEAN: Setter(write, error)) \/ Verify
Spec == Init /\ [][Next]_vars
AdmissionUsesReadSet == phase # "prepared" => admitted = matching
SetterPostcondition == phase = "done" => succeeded = (wrote /\ ~failed)
OtherEffectsPreserved == phase \in {"verify", "done"} => other = 2
=============================================================================

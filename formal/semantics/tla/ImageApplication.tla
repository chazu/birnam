------------------------ MODULE ImageApplication ------------------------
EXTENDS Naturals
CONSTANT Bug
VARIABLES phase, started, capable, backup, held, mapped, source, staged, installed
vars == <<phase, started, capable, backup, held, mapped, source, staged, installed>>
Init == /\ phase = "prepare" /\ started = FALSE /\ capable = TRUE
        /\ backup = FALSE /\ held \in BOOLEAN /\ mapped = TRUE
        /\ source = 0 /\ staged = TRUE /\ installed = FALSE
PreflightFail == /\ phase = "prepare" /\ phase' = "cleanup"
                 /\ UNCHANGED <<started, capable, backup, held, mapped, source, staged, installed>>
Begin == /\ phase = "prepare" /\ phase' \in {"apply", "cleanup"}
         /\ started' = TRUE /\ capable' = (Bug = "late")
         /\ backup' = (Bug = "backup")
         /\ mapped' = IF Bug = "code" THEN FALSE ELSE mapped
         /\ UNCHANGED <<held, source, staged, installed>>
Apply == /\ phase = "apply" /\ phase' = "cleanup"
         /\ source' = 1 /\ staged' = FALSE /\ installed' = TRUE
         /\ UNCHANGED <<started, capable, backup, held, mapped>>
Cleanup == /\ phase = "cleanup" /\ phase' = "collect"
           /\ capable' = IF started THEN Bug = "revive" ELSE capable
           /\ backup' = FALSE /\ staged' = FALSE
           /\ source' = IF Bug = "undo" THEN 0 ELSE source
           /\ UNCHANGED <<started, held, mapped, installed>>
Collect == /\ phase = "collect" /\ phase' = "done"
           /\ mapped' = (capable \/ backup \/ held)
           /\ UNCHANGED <<started, capable, backup, held, source, staged, installed>>
Next == PreflightFail \/ Begin \/ Apply \/ Cleanup \/ Collect
Spec == Init /\ [][Next]_vars
PreflightRetainsAuthority == ~started => capable
AuthorityReleasedAtBegin == started => ~capable
NoRecoveryBackup == ~backup
HeldCodeLives == held => mapped
InstalledSourcesRemain == installed => source = 1
StageReleased == phase \in {"collect", "done"} => ~staged
UnownedCodeReclaimed == phase = "done" /\ started /\ ~held => ~mapped
=============================================================================

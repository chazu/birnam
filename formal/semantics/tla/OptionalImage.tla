------------------------- MODULE OptionalImage -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "ambient", "missing", "cell"}
VARIABLES phase, selected, available, cellOwner, calls, data, applied
vars == <<phase,selected,available,cellOwner,calls,data,applied>>
Init == /\ phase = "prepare" /\ selected \in {0,1} /\ available \in BOOLEAN
        /\ cellOwner \in {0,1,2} /\ calls = 0 /\ data = 7 /\ applied = FALSE
Prepare == /\ phase = "prepare"
           /\ phase' = IF (selected = 0 \/ available \/ Bug = "missing")
                         /\ (cellOwner = 0 \/ cellOwner = selected \/ Bug = "cell")
                       THEN "apply" ELSE "rejected"
           /\ calls' = IF selected = 1 \/ Bug = "ambient" THEN 1 ELSE 0
           /\ UNCHANGED <<selected,available,cellOwner,data,applied>>
Apply == /\ phase = "apply" /\ phase' = "done" /\ applied' = TRUE
         /\ data' = IF selected = 1 THEN 9 ELSE data
         /\ UNCHANGED <<selected,available,cellOwner,calls>>
Next == Prepare \/ Apply
Spec == Init /\ [][Next]_vars
CoreIndependent == selected = 0 => calls = 0 /\ data = 7
ProviderPresent == phase \in {"apply","done"} /\ selected = 1 => available
CellsCovered == phase \in {"apply","done"} => cellOwner = 0 \/ cellOwner = selected
=======================================================================

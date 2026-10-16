-------------------------- MODULE ImageInverse --------------------------
EXTENDS Naturals, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none", "dictionary", "store", "sources", "globals", "replay"}
Owners == {"dictionary", "store", "sources", "globals"}
VARIABLES phase, values, versions, changed, callbacks
vars == <<phase, values, versions, changed, callbacks>>
Init == /\ phase = "callback" /\ values = [o \in Owners |-> 2]
        /\ versions = [o \in Owners |-> 1] /\ changed = {} /\ callbacks = 0
Callback == /\ phase = "callback" /\ phase' = "undo"
            /\ changed' \in SUBSET Owners
            /\ values' = [o \in Owners |-> IF o \in changed' THEN 3 ELSE values[o]]
            /\ versions' = [o \in Owners |-> IF o \in changed' THEN 2 ELSE versions[o]]
            /\ callbacks' = IF Bug = "replay" THEN 2 ELSE 1
Undo == /\ phase = "undo" /\ phase' = "done"
        /\ values' = [o \in Owners |->
             IF o = "globals" THEN (IF Bug = "globals" THEN 1 ELSE values[o])
             ELSE IF (versions[o] = 1 /\ values[o] = 2) \/ Bug = o THEN 1 ELSE values[o]]
        /\ UNCHANGED <<versions, changed, callbacks>>
Next == Callback \/ Undo
Spec == Init /\ [][Next]_vars
InterveningWritesSurvive == \A o \in changed : values[o] = 3
NoReplay == callbacks <= 1
=============================================================================

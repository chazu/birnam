-------------------------- MODULE ModuleExchange --------------------------
EXTENDS Naturals
CONSTANT Bug
VARIABLES phase, selected, outside, committed, beforeUndo, undone, expectedOutside
vars == <<phase, selected, outside, committed, beforeUndo, undone, expectedOutside>>
Init == /\ phase = "prepared" /\ selected = 0 /\ outside = 0
        /\ committed = FALSE /\ beforeUndo = 0 /\ undone = FALSE /\ expectedOutside = 0
Other == /\ phase \in {"prepared", "active"} /\ outside = 0 /\ outside' = 1
         /\ UNCHANGED <<phase, selected, committed, beforeUndo, undone, expectedOutside>>
Commit == /\ phase = "prepared" /\ committed' = (Bug # "lease" \/ outside = 0)
          /\ phase' = "active" /\ selected' = IF committed' THEN 1 ELSE selected
          /\ UNCHANGED <<outside, beforeUndo, undone, expectedOutside>>
Foreign == /\ phase = "active" /\ selected = 1 /\ selected' = 2
           /\ UNCHANGED <<phase, outside, committed, beforeUndo, undone, expectedOutside>>
Undo == /\ phase = "active" /\ phase' = "done" /\ beforeUndo' = selected /\ expectedOutside' = outside
        /\ undone' = (selected = 1 \/ Bug = "conflict")
        /\ selected' = IF undone' THEN 0 ELSE selected
        /\ outside' = IF Bug = "whole" THEN 0 ELSE outside
        /\ UNCHANGED committed
Next == Other \/ Commit \/ Foreign \/ Undo
Spec == Init /\ [][Next]_vars
OtherSourceSurvives == phase = "done" => outside = expectedOutside
ConditionalUndo == phase = "done" /\ beforeUndo = 2 => selected = 2 /\ ~undone
IndependentCommit == phase \in {"active", "done"} => committed
=============================================================================

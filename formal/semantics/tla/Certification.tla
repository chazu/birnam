-------------------------- MODULE Certification --------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "startup", "seal", "poison", "sticky"}
VARIABLES phase, adopted, requested, binding, source, baselineSource,
          undo, status, poisoned, checked, matchesNow, reported, introduced
vars == <<phase, adopted, requested, binding, source, baselineSource,
          undo, status, poisoned, checked, matchesNow, reported, introduced>>
Init == /\ phase = "start" /\ adopted = FALSE /\ requested = FALSE
        /\ binding = 1 /\ source = 0 /\ baselineSource = 0
        /\ undo = TRUE /\ status = "active" /\ poisoned = FALSE
        /\ checked = FALSE /\ matchesNow = FALSE /\ reported = FALSE /\ introduced = FALSE
Start == /\ phase = "start" /\ phase' = "ready" /\ adopted' = (Bug = "startup")
         /\ UNCHANGED <<requested, binding, source, baselineSource, undo, status, poisoned, checked, matchesNow, reported, introduced>>
Adopt == /\ phase = "ready" /\ ~requested /\ requested' = TRUE /\ adopted' = TRUE
         /\ baselineSource' = source /\ checked' = FALSE
         /\ UNCHANGED <<phase, binding, source, undo, status, poisoned, matchesNow, reported, introduced>>
Register == /\ phase = "ready" /\ ~introduced /\ introduced' = TRUE
            /\ source' = (IF Bug = "seal" /\ adopted THEN source ELSE 1)
            /\ checked' = FALSE
            /\ UNCHANGED <<phase, adopted, requested, binding, baselineSource, undo, status, poisoned, matchesNow, reported>>
Edit == /\ phase = "ready" /\ binding' = 3 - binding /\ checked' = FALSE
        /\ UNCHANGED <<phase, adopted, requested, source, baselineSource, undo, status, poisoned, matchesNow, reported, introduced>>
Validate == /\ phase = "ready" /\ checked' = TRUE
            /\ matchesNow' = (adopted /\ binding = 1 /\ source = baselineSource)
            /\ reported' = (matchesNow' /\ ~(Bug = "sticky" /\ poisoned))
            /\ poisoned' = (poisoned \/ ~matchesNow')
            /\ undo' = (IF Bug = "poison" /\ ~matchesNow' THEN FALSE ELSE undo)
            /\ status' = (IF Bug = "poison" /\ ~matchesNow' THEN "drifted" ELSE status)
            /\ UNCHANGED <<phase, adopted, requested, binding, source, baselineSource, introduced>>
Next == Start \/ Adopt \/ Register \/ Edit \/ Validate
Spec == Init /\ [][Next]_vars
NoImplicitAdoption == adopted => requested
SourceIntroduction == introduced => source = 1
ObservationPreservesState == undo /\ status = "active"
ReportsCurrentAgreement == checked => reported = matchesNow
=============================================================================

-------------------------- MODULE SourceOwner --------------------------
EXTENDS Naturals
CONSTANT Bug
VARIABLES phase, current, records, outside, journal, wrote, conflict, error, committed, observedOutside
vars == <<phase, current, records, outside, journal, wrote, conflict, error, committed, observedOutside>>
Init == /\ phase = "send" /\ current = 1 /\ records = [o \in 1..2 |-> IF o = 1 THEN 0 ELSE 3]
        /\ outside = 0 /\ journal = FALSE /\ wrote = FALSE /\ conflict = FALSE
        /\ error = FALSE /\ committed = FALSE /\ observedOutside = 0
Send == /\ phase = "send" /\ phase' = "callback"
        /\ journal' = (Bug # "journal") /\ wrote' \in BOOLEAN
        /\ records' = [records EXCEPT ![1] = IF wrote' THEN 1 ELSE @]
        /\ error' = ~wrote'
        /\ UNCHANGED <<current, outside, conflict, committed, observedOutside>>
Callback == /\ phase = "callback" /\ phase' = "check"
            /\ current' \in {1, 2} /\ outside' \in {0, 1}
            /\ conflict' \in BOOLEAN
            /\ records' = [records EXCEPT ![1] = IF conflict' THEN 2 ELSE @]
            /\ error' \in {error, TRUE}
            /\ observedOutside' = outside'
            /\ UNCHANGED <<journal, wrote, committed>>
Finish == /\ phase = "check" /\ phase' = "done"
          /\ committed' = (~error /\ current = 1 /\ records[1] = 1)
          /\ LET target == IF Bug = "owner" THEN current ELSE 1
                  undo == journal /\ (records[target] = 1 \/ Bug = "conflict" \/ (Bug = "owner" /\ current = 2))
              IN records' = IF committed' THEN records ELSE
                   [records EXCEPT ![target] = IF undo THEN 0 ELSE @]
          /\ outside' = IF Bug = "whole" /\ ~committed' THEN 0 ELSE outside
          /\ UNCHANGED <<current, journal, wrote, conflict, error, observedOutside>>
Next == Send \/ Callback \/ Finish
Spec == Init /\ [][Next]_vars
OtherOwnerPreserved == records[2] = 3
ConflictPreserved == conflict => records[1] = 2
CompensatesAfterError == phase = "done" /\ error /\ wrote /\ ~conflict => records[1] = 0
UnrelatedRegistrationPreserved == phase = "done" => outside = observedOutside
SuccessIsExact == committed => current = 1 /\ records[1] = 1
=============================================================================

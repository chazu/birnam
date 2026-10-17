---------------------------- MODULE ScopedEdit ----------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "whole", "conflict", "undo", "certificate"}
VARIABLES phase, selected, outside, observed, admitted, expectedAdmission,
          preservedOutside, certified
vars == <<phase, selected, outside, observed, admitted, expectedAdmission,
          preservedOutside, certified>>
Init == /\ phase = "prepared" /\ selected = 1 /\ outside = 1 /\ observed = 1
        /\ admitted = FALSE /\ expectedAdmission = FALSE
        /\ preservedOutside = 1 /\ certified = FALSE
OtherEdit == /\ phase \in {"prepared", "installed"} /\ outside = 1
             /\ outside' = 2
             /\ UNCHANGED <<phase, selected, observed, admitted, expectedAdmission, preservedOutside, certified>>
Conflict == /\ phase \in {"prepared", "installed"} /\ selected # 3
            /\ selected' = 3
            /\ UNCHANGED <<phase, outside, observed, admitted, expectedAdmission, preservedOutside, certified>>
Apply == /\ phase = "prepared"
         /\ expectedAdmission' = (selected = observed)
         /\ admitted' = ((selected = observed \/ Bug = "conflict") /\ (Bug # "whole" \/ outside = 1))
         /\ selected' = (IF admitted' THEN 2 ELSE selected)
         /\ phase' = (IF admitted' THEN "installed" ELSE "rejected")
         /\ UNCHANGED <<outside, observed, preservedOutside, certified>>
Undo == /\ phase = "installed"
        /\ expectedAdmission' = (selected = 2)
        /\ admitted' = ((selected = 2 \/ Bug = "conflict") /\ (Bug # "whole" \/ outside = 1))
        /\ selected' = (IF admitted' THEN observed ELSE selected)
        /\ preservedOutside' = outside
        /\ outside' = (IF Bug = "undo" THEN 1 ELSE outside)
        /\ certified' = (Bug = "certificate" \/ (selected' = observed /\ outside' = 1))
        /\ phase' = "done" /\ UNCHANGED observed
Next == OtherEdit \/ Conflict \/ Apply \/ Undo
Spec == Init /\ [][Next]_vars
ScopedAdmission == phase # "prepared" => admitted = expectedAdmission
OutsidePreserved == phase = "done" => outside = preservedOutside
HonestCertificate == phase = "done" /\ certified => selected = observed /\ outside = 1
=============================================================================

------------------------ MODULE EnteredActivation ------------------------
EXTENDS Naturals
CONSTANT Bug
VARIABLES phase, active, frame, binding, retired, installed
vars == <<phase, active, frame, binding, retired, installed>>
Init == /\ phase = "entered" /\ active = TRUE /\ frame = 1
        /\ binding = 1 /\ retired = FALSE /\ installed = FALSE
Install == /\ phase = "entered" /\ phase' = "installed"
           /\ installed' = (Bug # "idle") /\ binding' = IF installed' THEN 2 ELSE binding
           /\ frame' = IF Bug = "frame" THEN 2 ELSE frame
           /\ UNCHANGED <<active, retired>>
Collect == /\ phase = "installed" /\ phase' = "collected"
           /\ retired' = (Bug = "lease") /\ UNCHANGED <<active, frame, binding, installed>>
Return == /\ phase = "collected" /\ phase' = "done" /\ active' = FALSE
          /\ UNCHANGED <<frame, binding, retired, installed>>
Next == Install \/ Collect \/ Return
Spec == Init /\ [][Next]_vars
OrdinaryAdmission == phase # "entered" => installed /\ binding = 2
EnteredCodeLives == active => ~retired
EnteredBodyUnchanged == frame = 1
=============================================================================

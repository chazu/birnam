------------------------- MODULE SharedInvocation -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "cached", "shared-state", "unrooted", "replay", "no-frame", "native-barrier"}
VARIABLES phase, binding, entered, version, selectedVersion, callback,
          alive, calls, frame, suspended
vars == <<phase, binding, entered, version, selectedVersion, callback,
          alive, calls, frame, suspended>>
Init == /\ phase = "enter" /\ binding = 1 /\ entered = 0 /\ version = 0
        /\ selectedVersion = 0 /\ callback = 0 /\ alive = TRUE /\ calls = 0
        /\ frame = FALSE /\ suspended = FALSE
Enter == /\ phase = "enter" /\ entered' = binding /\ phase' = "waiting"
         /\ frame' = (Bug # "no-frame")
         /\ UNCHANGED <<binding, version, selectedVersion, callback, alive, calls, suspended>>
ReplaceBinding == /\ phase = "waiting" /\ binding = 1 /\ binding' = 2
                  /\ UNCHANGED <<phase, entered, version, selectedVersion, callback, alive, calls, frame, suspended>>
ReplaceInvocation == /\ phase = "waiting" /\ version = 0 /\ version' = 1
                     /\ UNCHANGED <<phase, binding, entered, selectedVersion, callback, alive, calls, frame, suspended>>
Collect == /\ phase = "waiting"
           /\ alive' = (binding = entered \/ Bug # "unrooted")
           /\ UNCHANGED <<phase, binding, entered, version, selectedVersion, callback, calls, frame, suspended>>
Invoke == /\ phase = "waiting" /\ phase' = "body"
          /\ callback' = IF Bug = "shared-state" THEN 2 ELSE entered
          /\ selectedVersion' = IF Bug = "cached" THEN 0 ELSE version
          /\ calls' = IF Bug = "replay" THEN 2 ELSE 1
          /\ frame' = FALSE
          /\ UNCHANGED <<binding, entered, version, alive, suspended>>
Suspend == /\ phase = "body" /\ phase' = "parked"
           /\ suspended' = (Bug # "native-barrier")
           /\ UNCHANGED <<binding, entered, version, selectedVersion, callback, alive, calls, frame>>
Finish == /\ phase = "parked" /\ phase' = "done"
          /\ UNCHANGED <<binding, entered, version, selectedVersion, callback, alive, calls, frame, suspended>>
Next == Enter \/ ReplaceBinding \/ ReplaceInvocation \/ Collect \/ Invoke \/ Suspend \/ Finish
Spec == Init /\ [][Next]_vars
EnteredBindingRetained == phase \in {"body", "parked", "done"} => callback = entered
LookupIsCurrent == phase \in {"body", "parked", "done"} => selectedVersion = version
EnteredOwnerAlive == phase # "enter" => alive
NoReplay == calls <= 1
LogicalFrameVisible == phase = "waiting" => frame
SuspensionPreserved == phase \in {"parked", "done"} => suspended
=======================================================================

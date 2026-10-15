------------------------- MODULE RecoveryDrive -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none","epoch","action","scope","frame","lease"}
VARIABLES phase, epoch, delimiter, frame, lease
vars == <<phase,epoch,delimiter,frame,lease>>
Init == /\ phase = "ready" /\ epoch = 0 /\ delimiter = 1 /\ frame = 1 /\ lease = TRUE
Enter == /\ phase = "ready" /\ phase' = "nested" /\ epoch' = 1 /\ delimiter' = 2 /\ UNCHANGED <<frame,lease>>
Return == /\ phase = "nested" /\ phase' = "returned"
          /\ epoch' = IF Bug = "action" THEN 0 ELSE 2
          /\ delimiter' = 1 /\ UNCHANGED <<frame,lease>>
Scope == /\ phase = "returned" /\ phase' = "scope" /\ delimiter' = 3 /\ UNCHANGED <<epoch,frame,lease>>
Frame == /\ phase = "returned" /\ phase' = "frame" /\ frame' = 2 /\ UNCHANGED <<epoch,delimiter,lease>>
Revoke == /\ phase = "returned" /\ phase' = "revoked" /\ lease' = FALSE /\ UNCHANGED <<epoch,delimiter,frame>>
Next == Enter \/ Return \/ Scope \/ Frame \/ Revoke
Spec == Init /\ [][Next]_vars
PrivateValid == /\ (Bug = "scope" \/ delimiter = 1)
                /\ (Bug = "frame" \/ frame = 1)
                /\ (Bug = "lease" \/ lease)
                /\ (Bug # "epoch" \/ epoch = 0)
BalancedReentry == phase = "returned" => PrivateValid
OldActionsStayStale == phase = "returned" => epoch # 0
NewScopeInvalidates == phase = "scope" => ~PrivateValid
ChangedFrameInvalidates == phase = "frame" => ~PrivateValid
RevokedLeaseInvalidates == phase = "revoked" => ~PrivateValid
=======================================================================

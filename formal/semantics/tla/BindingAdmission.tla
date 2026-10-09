------------------------ MODULE BindingAdmission ------------------------
EXTENDS Naturals, Sequences
(* Two target entries model the GC batch; service replacement forces legacy callbacks. *)
CONSTANT Bug
ASSUME Bug \in {"none", "ignore-context", "ignore-services", "cache-permission", "ignore-epoch"}
VARIABLES method, context, method2, context2, defaultLookup, services, legacyVerdict, permission,
          epoch, capturedEpoch, stage, admitted, fast, referenceVerdict,
          optimizedVerdict, referenceTrace, optimizedTrace
vars == <<method,context,method2,context2,defaultLookup,services,legacyVerdict,permission,epoch,
          capturedEpoch,stage,admitted,fast,referenceVerdict,optimizedVerdict,
          referenceTrace,optimizedTrace>>
Init == /\ method \in {0,1} /\ context \in {0,1}
        /\ method2 \in {0,1} /\ context2 \in {0,1} /\ defaultLookup \in BOOLEAN
        /\ services \in {0,1} /\ legacyVerdict \in BOOLEAN /\ permission = TRUE
        /\ epoch = 0 /\ capturedEpoch = 0 /\ stage = "guard" /\ admitted = FALSE
        /\ fast = FALSE /\ referenceVerdict = FALSE /\ optimizedVerdict = FALSE
        /\ referenceTrace = <<>> /\ optimizedTrace = <<>>
CurrentMatch == defaultLookup /\ method = 0 /\ context = 0
                /\ method2 = 0 /\ context2 = 0
OrdinaryVerdict == IF services = 0 THEN CurrentMatch ELSE legacyVerdict
OrdinaryTrace == IF services = 0 THEN <<>> ELSE <<"legacy-guard-callback">>
Packed == defaultLookup /\ method = 0 /\ method2 = 0
          /\ (Bug = "ignore-context" \/ (context = 0 /\ context2 = 0))
          /\ (Bug = "ignore-services" \/ services = 0)
Guard == /\ stage = "guard" /\ stage' = "callback"
         /\ capturedEpoch' = epoch /\ admitted' = Packed
         /\ referenceVerdict' = OrdinaryVerdict /\ referenceTrace' = OrdinaryTrace
         /\ optimizedVerdict' = (IF Packed THEN TRUE ELSE OrdinaryVerdict)
         /\ optimizedTrace' = (IF Packed THEN <<>> ELSE OrdinaryTrace)
         /\ UNCHANGED <<method,context,method2,context2,defaultLookup,services,legacyVerdict,permission,epoch,fast>>
(* Signature/permission access remains a boundary, after the guard. *)
Callback(m,c,m2,c2,p) == /\ stage = "callback" /\ stage' = "adapt"
                  /\ m \in {0,1} /\ c \in {0,1} /\ m2 \in {0,1} /\ c2 \in {0,1} /\ p \in BOOLEAN
                  /\ method' = m /\ context' = c /\ method2' = m2 /\ context2' = c2 /\ permission' = p
                  /\ epoch' = (IF <<m,c,m2,c2>> = <<method,context,method2,context2>> THEN epoch ELSE epoch+1)
                  /\ UNCHANGED <<defaultLookup,services,legacyVerdict,capturedEpoch,
                                 admitted,fast,referenceVerdict,optimizedVerdict,
                                 referenceTrace,optimizedTrace>>
Adapt == /\ stage = "adapt" /\ stage' = "done"
         /\ fast' = (admitted /\ (Bug = "cache-permission" \/ permission)
                    /\ (Bug = "ignore-epoch" \/ capturedEpoch = epoch))
         /\ UNCHANGED <<method,context,method2,context2,defaultLookup,services,legacyVerdict,permission,
                        epoch,capturedEpoch,admitted,referenceVerdict,optimizedVerdict,
                        referenceTrace,optimizedTrace>>
Next == Guard \/ (\E m,c,m2,c2 \in {0,1}, p \in BOOLEAN : Callback(m,c,m2,c2,p)) \/ Adapt
Spec == Init /\ [][Next]_vars
SameGuard == stage # "guard" => referenceVerdict = optimizedVerdict
SameGuardCallbacks == referenceTrace = optimizedTrace
CurrentEntry == fast => CurrentMatch
CurrentPermission == fast => permission
==========================================================================

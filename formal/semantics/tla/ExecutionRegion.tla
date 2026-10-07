------------------------ MODULE ExecutionRegion -------------------------
EXTENDS Naturals, Sequences, FiniteSets
CONSTANTS Plan, MaxEpoch, MaxBoundaries, Bug
ASSUME /\ Len(Plan)>0 /\ MaxEpoch>0 /\ MaxBoundaries>0
       /\ Bug \in {"none","stale-dispatch","epoch-only","observe-before-recover",
                    "replay-prefix","lost-code-root","foreign-payload"}
Addition == <<1,1,2,2,2,2,2,1>>
StringLength == <<3,1>>
CollectionAccess == <<1,4>>
Keys == {1,2,3,4}
Deps == {Plan[i] : i \in 1..Len(Plan)}
VARIABLES meanings, permission, epoch, captured, capturedEpoch, mode, stage,
          pc, referenceValue, optimizedValue, physicalValue, physicalPC,
          referenceTrace, optimizedTrace, boundaries, activeGeneration,
          currentGeneration, codeLease, heap, badOwner, referenceSends, optimizedSends
vars == <<meanings,permission,epoch,captured,capturedEpoch,mode,stage,pc,
          referenceValue,optimizedValue,physicalValue,physicalPC,
          referenceTrace,optimizedTrace,boundaries,activeGeneration,
          currentGeneration,codeLease,heap,badOwner,referenceSends,optimizedSends>>
Init == /\ meanings = [k \in Keys |-> 0] /\ permission = TRUE /\ epoch = 0
        /\ captured = meanings /\ capturedEpoch = 0 /\ mode = "none" /\ stage = "guard"
        /\ pc = 0 /\ referenceValue = 0 /\ optimizedValue = 0
        /\ physicalValue = 0 /\ physicalPC = 0
        /\ referenceTrace = <<>> /\ optimizedTrace = <<>> /\ boundaries = 0
        /\ activeGeneration = 0 /\ currentGeneration = 0 /\ codeLease = TRUE
        /\ heap = {1,2,3,4} /\ badOwner = FALSE
        /\ referenceSends = 0 /\ optimizedSends = 0
Valid == permission /\ \A k \in Deps : meanings[k] = captured[k]
Guard == /\ stage = "guard"
         /\ captured' = meanings /\ capturedEpoch' = epoch
         /\ mode' = IF permission THEN "optimized" ELSE "baseline"
         /\ stage' = "run" /\ optimizedSends' = optimizedSends+1
         /\ physicalValue' = optimizedValue /\ physicalPC' = pc
         /\ UNCHANGED <<meanings,permission,epoch,pc,referenceValue,optimizedValue,
                        referenceTrace,optimizedTrace,boundaries,activeGeneration,
                        currentGeneration,codeLease,heap,badOwner,referenceSends>>
(* The vocabulary permits one owner message to execute positive-count
   consecutive private operations at that owner. It permits no foreign-payload
   read by the policy caller. This is an abstract kernel, not total Birnam cost. *)
Step(n) == /\ stage = "run" /\ pc < Len(Plan) /\ n \in 1..(Len(Plan)-pc)
           /\ IF mode = "optimized" THEN
                 \A i \in (pc+1)..(pc+n) : Plan[i] = Plan[pc+1]
              ELSE n = 1
           /\ referenceValue' = referenceValue+n*(meanings[Plan[pc+1]]+1)
           /\ optimizedValue' = optimizedValue+n*((IF mode = "optimized" THEN captured ELSE meanings)[Plan[pc+1]]+1)
           /\ pc' = pc+n
           /\ physicalValue' = IF mode = "baseline" THEN optimizedValue' ELSE physicalValue
           /\ physicalPC' = IF mode = "baseline" THEN pc' ELSE physicalPC
           /\ badOwner' = IF Bug = "foreign-payload" THEN TRUE ELSE badOwner
           /\ referenceSends' = referenceSends+n
           /\ optimizedSends' = optimizedSends+1
           /\ UNCHANGED <<meanings,permission,epoch,captured,capturedEpoch,mode,stage,
                          referenceTrace,optimizedTrace,boundaries,activeGeneration,
                          currentGeneration,codeLease,heap>>
Boundary == /\ stage = "run" /\ pc>0 /\ boundaries < MaxBoundaries
            /\ stage' = "expose" /\ mode' = "none" /\ boundaries' = boundaries+1
            /\ physicalValue' = IF Bug = "observe-before-recover" THEN physicalValue
                                 ELSE IF Bug = "replay-prefix" THEN 0 ELSE optimizedValue
            /\ physicalPC' = IF Bug = "observe-before-recover" THEN physicalPC ELSE pc
            /\ optimizedSends' = optimizedSends+1
            /\ UNCHANGED <<meanings,permission,epoch,captured,capturedEpoch,pc,
                           referenceValue,optimizedValue,referenceTrace,optimizedTrace,
                           activeGeneration,currentGeneration,codeLease,heap,badOwner,referenceSends>>
Observe == /\ stage = "expose" /\ stage' = "callback"
           /\ referenceTrace' = Append(referenceTrace,<<referenceValue,pc,activeGeneration>>)
           /\ optimizedTrace' = Append(optimizedTrace,<<physicalValue,physicalPC,activeGeneration>>)
           /\ UNCHANGED <<meanings,permission,epoch,captured,capturedEpoch,mode,pc,
                          referenceValue,optimizedValue,physicalValue,physicalPC,boundaries,
                          activeGeneration,currentGeneration,codeLease,heap,badOwner,
                          referenceSends,optimizedSends>>
Replace(k) == /\ stage = "callback" /\ epoch < MaxEpoch /\ k \in Keys
              /\ meanings' = [meanings EXCEPT ![k] = 1-@] /\ epoch' = epoch+1
              /\ currentGeneration' = 1-currentGeneration
              /\ codeLease' = IF Bug = "lost-code-root" THEN FALSE ELSE codeLease
              /\ UNCHANGED <<permission,captured,capturedEpoch,mode,stage,pc,referenceValue,
                             optimizedValue,physicalValue,physicalPC,referenceTrace,optimizedTrace,
                             boundaries,activeGeneration,heap,badOwner,referenceSends,optimizedSends>>
Revoke == /\ stage = "callback" /\ permission /\ permission' = FALSE
          /\ UNCHANGED <<meanings,epoch,captured,capturedEpoch,mode,stage,pc,referenceValue,
                         optimizedValue,physicalValue,physicalPC,referenceTrace,optimizedTrace,
                         boundaries,activeGeneration,currentGeneration,codeLease,heap,badOwner,
                         referenceSends,optimizedSends>>
Resume == /\ stage = "callback"
          /\ stage' = IF Bug = "stale-dispatch" \/
                           (Bug = "epoch-only" /\ capturedEpoch = epoch) THEN "run" ELSE "guard"
          /\ mode' = IF stage' = "run" THEN "optimized" ELSE "none"
          /\ UNCHANGED <<meanings,permission,epoch,captured,capturedEpoch,pc,referenceValue,
                         optimizedValue,physicalValue,physicalPC,referenceTrace,optimizedTrace,
                         boundaries,activeGeneration,currentGeneration,codeLease,heap,badOwner,
                         referenceSends,optimizedSends>>
Collect == /\ heap' = heap \cap ({1,2,3+currentGeneration} \cup
                                 IF codeLease THEN {3+activeGeneration} ELSE {})
           /\ UNCHANGED <<meanings,permission,epoch,captured,capturedEpoch,mode,stage,pc,
                          referenceValue,optimizedValue,physicalValue,physicalPC,referenceTrace,
                          optimizedTrace,boundaries,activeGeneration,currentGeneration,codeLease,
                          badOwner,referenceSends,optimizedSends>>
TypeOK == /\ meanings \in [Keys -> 0..1] /\ captured \in [Keys -> 0..1]
          /\ permission \in BOOLEAN /\ epoch \in 0..MaxEpoch
          /\ pc \in 0..Len(Plan) /\ activeGeneration = 0
          /\ currentGeneration \in 0..1 /\ heap \subseteq {1,2,3,4}
          /\ codeLease \in BOOLEAN /\ badOwner \in BOOLEAN
          /\ boundaries \in 0..MaxBoundaries
Next == Guard \/ (\E n \in 1..Len(Plan) : Step(n)) \/ Boundary \/ Observe \/
        (\E k \in Keys : Replace(k)) \/ Revoke \/ Resume \/ Collect
Spec == Init /\ [][Next]_vars
CurrentMeaning == stage = "run" /\ mode = "optimized" => Valid
SameBehavior == referenceValue = optimizedValue /\ referenceTrace = optimizedTrace
RecoveredBeforeExposure == stage \in {"expose","callback"} =>
                             physicalValue = referenceValue /\ physicalPC = pc
EnteredCodeAlive == 3+activeGeneration \in heap
OwnerBoundary == ~badOwner
Limit == referenceSends <= 12 /\ optimizedSends <= 18
=============================================================================

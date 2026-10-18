------------------------- MODULE CertificateRelease -------------------------
EXTENDS Naturals
CONSTANT Bug
VARIABLES phase, certificateRoot, held, mapped, binding, undo, description
vars == <<phase, certificateRoot, held, mapped, binding, undo, description>>
Init == /\ phase = 0 /\ certificateRoot = TRUE /\ held \in BOOLEAN
        /\ mapped = TRUE /\ binding = 2 /\ undo = TRUE /\ description = 1
Release == /\ phase = 0 /\ phase' = 1 /\ certificateRoot' = (Bug = "roots")
           /\ mapped' = IF Bug = "code" THEN FALSE ELSE mapped
           /\ undo' = IF Bug = "undo" THEN FALSE ELSE undo
           /\ description' = IF Bug = "description" THEN 0 ELSE description
           /\ UNCHANGED <<held, binding>>
Collect == /\ phase = 1 /\ phase' = 2 /\ mapped' = (certificateRoot \/ held)
           /\ UNCHANGED <<certificateRoot, held, binding, undo, description>>
Next == Release \/ Collect
Spec == Init /\ [][Next]_vars
ObservationReleased == phase > 0 => ~certificateRoot
OwnersPreserved == binding = 2 /\ undo
DescriptionPreserved == description = 1
IndependentCodeLives == held => mapped
UnownedCodeReclaimed == phase = 2 /\ ~held => ~mapped
=============================================================================

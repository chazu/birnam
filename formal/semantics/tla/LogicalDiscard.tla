------------------------- MODULE LogicalDiscard -------------------------
EXTENDS Naturals
CONSTANT Bug
VARIABLES phase, handle, prepared, inverse, files, binding, entered, retained
vars == <<phase, handle, prepared, inverse, files, binding, entered, retained>>
Init == /\ phase = "open" /\ handle = TRUE /\ prepared = TRUE /\ inverse = TRUE
        /\ files = 1 /\ binding = 1 /\ entered = TRUE /\ retained = TRUE
Discard == /\ phase = "open" /\ phase' = "discarded" /\ handle' = FALSE /\ inverse' = FALSE
           /\ prepared' = (Bug = "token")
           /\ files' = IF Bug = "files" THEN 0 ELSE files
           /\ binding' = IF Bug = "binding" THEN 0 ELSE binding
           /\ retained' = (Bug # "code") /\ UNCHANGED entered
Next == Discard
Spec == Init /\ [][Next]_vars
FilesPreserved == files = 1
BindingPreserved == binding = 1
CapabilitiesRevoked == phase = "discarded" => ~handle /\ ~prepared /\ ~inverse
CodeLives == (entered \/ binding = 1) => retained
=============================================================================

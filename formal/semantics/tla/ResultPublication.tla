------------------------ MODULE ResultPublication ------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "overwrite", "late-expected", "implicit-description", "ignore-error", "replay"}
VARIABLES phase, dictionary, expected, opted, descriptions, callbacks, changed, result
vars == <<phase, dictionary, expected, opted, descriptions, callbacks, changed, result>>
Init == /\ phase = "commit" /\ dictionary = 1 /\ expected = 2
        /\ opted \in BOOLEAN /\ descriptions = 0 /\ callbacks = 0
        /\ changed = FALSE /\ result = "pending"
Commit == /\ phase = "commit" /\ dictionary' = 2 /\ phase' = "describe"
          /\ UNCHANGED <<expected, opted, descriptions, callbacks, changed, result>>
Describe == /\ phase = "describe" /\ phase' = "notify"
            /\ descriptions' = IF opted \/ Bug = "implicit-description" THEN 1 ELSE 0
            /\ UNCHANGED <<dictionary, expected, opted, callbacks, changed, result>>
Callback == /\ phase = "notify" /\ phase' = "undo"
            /\ dictionary' = 3 /\ changed' = TRUE
            /\ expected' = IF Bug = "late-expected" THEN 3 ELSE expected
            /\ callbacks' = IF Bug = "replay" THEN 2 ELSE 1
            /\ result' = IF Bug = "ignore-error" THEN "success" ELSE "error"
            /\ UNCHANGED <<opted, descriptions>>
Undo == /\ phase = "undo" /\ phase' = "done"
        /\ dictionary' = IF dictionary = expected \/ Bug = "overwrite" THEN 1 ELSE dictionary
        /\ UNCHANGED <<expected, opted, descriptions, callbacks, changed, result>>
Next == Commit \/ Describe \/ Callback \/ Undo
Spec == Init /\ [][Next]_vars
InterveningWriteSurvives == changed => dictionary = 3
DescriptionIsOptIn == ~opted => descriptions = 0
RequiredHookErrorVisible == phase \in {"undo", "done"} => result = "error"
NoCallbackReplay == callbacks <= 1
=============================================================================

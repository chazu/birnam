------------------------- MODULE MethodTemplates -------------------------
EXTENDS Naturals, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none", "stale", "overwrite", "binding-revision", "replay", "save-live", "persist-cache"}
VARIABLES phase, cache, before, expected, proposed, calls, entered, image, bindingBasis
vars == <<phase, cache, before, expected, proposed, calls, entered, image, bindingBasis>>
Init == /\ phase = "lookup" /\ cache = {} /\ before = {} /\ expected = {} /\ proposed = {}
        /\ calls = 0 /\ entered = 0 /\ image = {} /\ bindingBasis = 0
Miss == /\ phase = "lookup" /\ before' = cache /\ phase' = "compile"
        /\ calls' = 1 /\ UNCHANGED <<cache, expected, proposed, entered, image, bindingBasis>>
Reenter == /\ phase \in {"compile", "publish"} /\ cache = {} /\ cache' = {2}
           /\ UNCHANGED <<phase, before, expected, proposed, calls, entered, image, bindingBasis>>
ReadCurrent == /\ phase = "compile" /\ phase' = "publish" /\ expected' = cache
               /\ proposed' = (IF Bug = "stale" THEN before ELSE cache) \cup {1}
               /\ UNCHANGED <<cache, before, calls, entered, image, bindingBasis>>
Publish == /\ phase = "publish" /\ phase' = "bound"
           /\ cache' = IF Bug = "overwrite" \/ cache = expected THEN proposed ELSE cache
           /\ before' = cache /\ entered' = 1
           /\ calls' = IF Bug = "replay" THEN 2 ELSE calls
           /\ bindingBasis' = IF Bug = "binding-revision" THEN bindingBasis + 1 ELSE bindingBasis
           /\ UNCHANGED <<expected, proposed, image>>
Save == /\ phase = "bound" /\ phase' = "saved"
        /\ before' = cache
        /\ image' = IF Bug = "persist-cache" THEN cache ELSE {}
        /\ cache' = IF Bug = "save-live" THEN {} ELSE cache
        /\ UNCHANGED <<expected, proposed, calls, entered, bindingBasis>>
Next == Miss \/ Reenter \/ ReadCurrent \/ Publish \/ Save
Spec == Init /\ [][Next]_vars
KeepReenteredTemplate == phase = "bound" /\ 2 \in before => 2 \in cache
DoNotReplayCompiler == calls <= 1
SaveDoesNotClearLiveCache == phase = "saved" => cache = before
ImageExcludesMethods == image = {}
EnteredBindingSurvives == phase \in {"bound", "saved"} => entered = 1
PrivateCacheDoesNotPublishGlobals == bindingBasis = 0
=============================================================================

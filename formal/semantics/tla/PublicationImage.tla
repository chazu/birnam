------------------------- MODULE PublicationImage -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "mapped", "rebuilt"}
VARIABLES phase, mode, style, savedStyle, input, original, restored
vars == <<phase, mode, style, savedStyle, input, original, restored>>
Result(s, i) == IF i = "ordinary-error" THEN "error-value"
               ELSE IF s = "synchronous" THEN "native-failure" ELSE "error-value"
Init == /\ phase = "fresh" /\ mode \in {"mapped", "rebuilt"}
        /\ style = "synchronous" /\ savedStyle = "absent"
        /\ input \in {"native-failure", "ordinary-error"}
        /\ original = "unobserved" /\ restored = "unobserved"
ObserveFresh == /\ phase = "fresh" /\ original' = Result(style, input)
                /\ phase' = "save"
                /\ UNCHANGED <<mode, style, savedStyle, input, restored>>
Save == /\ phase = "save" /\ savedStyle' = style /\ phase' = "restore"
        /\ UNCHANGED <<mode, style, input, original, restored>>
Restore == /\ phase = "restore"
           /\ style' = IF Bug = mode THEN "resumable" ELSE savedStyle
           /\ phase' = "observe"
           /\ UNCHANGED <<mode, savedStyle, input, original, restored>>
ObserveRestored == /\ phase = "observe" /\ restored' = Result(style, input)
                   /\ phase' = "done"
                   /\ UNCHANGED <<mode, style, savedStyle, input, original>>
Next == ObserveFresh \/ Save \/ Restore \/ ObserveRestored
Spec == Init /\ [][Next]_vars
FailureChannelPreserved == phase = "done" => restored = original
OrdinaryErrorRemainsValue == phase = "done" /\ input = "ordinary-error" => restored = "error-value"
=======================================================================

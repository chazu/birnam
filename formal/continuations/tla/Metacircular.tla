---------------------------- MODULE Metacircular ----------------------------
EXTENDS Naturals, Sequences, FiniteSets, TLC
CONSTANTS Depth, NativeMode, Bug
ASSUME /\ Depth \in 2..3 /\ NativeMode \in {"sync", "async"}
       /\ Bug \in {"none", "early-publish", "no-anchor", "frozen-anchor",
                    "shortcut", "reselect", "lose-request", "lookup-replay",
                    "selected-lookup", "drop-lookup-code", "native-escape",
                    "native-tail", "double-completion", "overlap", "callback-tail"}
Methods == {"lookup0", "lookup1", "native0", "native1", "helperBody"}
LookupMethods == {"lookup0", "lookup1"}
None == "none"
RootRequest == [r |-> 0, s |-> "work", args |-> <<"argument">>, k |-> 0]
Vtable(r) == IF r < Depth THEN r + 1 ELSE Depth
Request(r, s, args, k) == [r |-> r, s |-> s, args |-> args, k |-> k]
\* Labels below are specification/owner state, not runtime payload tags.
\* Raw is the bootstrap vtable owner's operation; continuation records are
\* decoded by their owners in ResumeLookup/ResumeLookupHelper/ResumeNative.
VARIABLES boot, seeded, published, op, cur, selected, authority, value, frames,
          lookupBinding, workBinding, lookupChanged, workChanged, alive,
          parked, paused, effects, reads, baseReads, metaEntries, lookupEffects,
          nativeLive, foreignReturns, nativeAnswers, callbackDone, duplicateSeen,
          drivers, overlapSeen, denied, done, finalK, selectedLookups
vars == <<boot, seeded, published, op, cur, selected, authority, value, frames,
          lookupBinding, workBinding, lookupChanged, workChanged, alive,
          parked, paused, effects, reads, baseReads, metaEntries, lookupEffects,
          nativeLive, foreignReturns, nativeAnswers, callbackDone, duplicateSeen,
          drivers, overlapSeen, denied, done, finalK, selectedLookups>>
Raw(table, selector) ==
  IF selector = "lookup" THEN lookupBinding[table]
  ELSE IF table = 1 /\ selector = "work" THEN workBinding
  ELSE IF table = 1 /\ selector = "helper" THEN "helperBody" ELSE None
Top == frames[Len(frames)]
Pop == SubSeq(frames, 1, Len(frames)-1)
Save(kind, request, owner) == [kind |-> kind, request |-> request, owner |-> owner]
Init == /\ boot = 0 /\ seeded = FALSE /\ published = FALSE
        /\ op = "idle" /\ cur = RootRequest /\ selected = None /\ authority = None /\ value = None /\ frames = <<>>
        /\ lookupBinding = [level \in 1..Depth |-> "lookup0"] /\ workBinding = "native0"
        /\ lookupChanged = {} /\ workChanged = FALSE /\ alive = Methods
        /\ parked = FALSE /\ paused = FALSE /\ effects = 0 /\ reads = 0
        /\ baseReads = 0 /\ metaEntries = {} /\ lookupEffects = {}
        /\ nativeLive = 0 /\ foreignReturns = 0 /\ nativeAnswers = 0
        /\ callbackDone = FALSE /\ duplicateSeen = FALSE
        /\ drivers = 0 /\ overlapSeen = FALSE /\ denied = FALSE
        /\ done = FALSE /\ finalK = None /\ selectedLookups = 0

Bootstrap == /\ boot < 3 /\ boot' = boot + 1
             /\ seeded' = (seeded \/ boot = 1)
             /\ published' = (published \/ boot = (IF Bug = "early-publish" THEN 0 ELSE 2))
             /\ UNCHANGED <<op, cur, selected, authority, value, frames, lookupBinding, workBinding,
                 lookupChanged, workChanged, alive, parked, paused, effects, reads, baseReads,
                 metaEntries, lookupEffects, nativeLive, foreignReturns, nativeAnswers, callbackDone,
                 duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
Start == /\ published /\ op = "idle" /\ op' = "send" /\ drivers' = 1
         /\ UNCHANGED <<boot, seeded, published, cur, selected, authority, value, frames,
             lookupBinding, workBinding, lookupChanged, workChanged, alive, parked, paused,
             effects, reads, baseReads, metaEntries, lookupEffects, nativeLive, foreignReturns,
             nativeAnswers, callbackDone, duplicateSeen, overlapSeen, denied, done, finalK, selectedLookups>>
Send == /\ op = "send" /\ ~parked
        /\ IF (cur.r = Depth /\ cur.s = "lookup" /\ Bug # "no-anchor") \/ Bug = "shortcut"
            THEN /\ op' = "enter" /\ authority' = Raw(Vtable(cur.r), cur.s)
                 /\ selected' = IF Bug = "frozen-anchor" THEN "lookup0" ELSE Raw(Vtable(cur.r), cur.s)
                 /\ baseReads' = baseReads + 1 /\ UNCHANGED <<cur, frames>>
            ELSE /\ frames' = Append(frames, Save("lookup", cur, None))
                 /\ cur' = Request(Vtable(cur.r), "lookup", <<cur.s>>, Len(frames)+1)
                 /\ UNCHANGED <<op, selected, authority, baseReads>>
        /\ UNCHANGED <<boot, seeded, published, value, lookupBinding, workBinding,
             lookupChanged, workChanged, alive, parked, paused, effects, reads, metaEntries,
             lookupEffects, nativeLive, foreignReturns, nativeAnswers, callbackDone,
             duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
EnterLookup ==
    /\ op = "enter" /\ ~parked /\ selected \in LookupMethods
    /\ op' = (IF cur.r = 1 /\ cur.args = <<"work">> THEN "lookup-callback" ELSE "lookup-read")
    /\ effects' = effects + 1
    /\ lookupEffects' = lookupEffects \cup {selected}
    /\ metaEntries' = metaEntries \cup {cur.r}
    /\ selectedLookups' = selectedLookups + (IF Bug = "selected-lookup" THEN 1 ELSE 0)
    /\ paused' = FALSE
    /\ UNCHANGED <<boot, seeded, published, cur, selected, authority, value, frames,
        lookupBinding, workBinding, lookupChanged, workChanged, alive, parked,
        reads, baseReads, nativeLive, foreignReturns, nativeAnswers, callbackDone,
        duplicateSeen, drivers, overlapSeen, denied, done, finalK>>
CallLookupHelper ==
    /\ op = "lookup-callback" /\ ~parked
    /\ frames' = (IF Bug = "callback-tail" THEN frames ELSE Append(frames, Save("lookup-resume", cur, selected)))
    /\ cur' = Request(0, "helper", <<"argument">>, IF Bug = "callback-tail" THEN cur.k ELSE Len(frames)+1)
    /\ op' = "send"
    /\ UNCHANGED <<boot, seeded, published, selected, authority, value, lookupBinding, workBinding, lookupChanged, workChanged, alive, parked, paused, effects, reads, baseReads, metaEntries, lookupEffects, nativeLive, foreignReturns, nativeAnswers, callbackDone, duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
ResumeLookupHelper ==
    /\ op = "return" /\ cur.k > 0 /\ Top.kind = "lookup-resume"
    /\ cur' = Top.request /\ frames' = Pop
    /\ selected' = Top.owner /\ authority' = Top.owner /\ op' = "lookup-read"
    /\ paused' = FALSE
    /\ UNCHANGED <<boot, seeded, published, value, lookupBinding, workBinding, lookupChanged, workChanged, alive, parked, effects, reads, baseReads, metaEntries, lookupEffects, nativeLive, foreignReturns, nativeAnswers, callbackDone, duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
LookupRead == /\ op = "lookup-read" /\ ~parked
              /\ value' = Raw(cur.r, cur.args[1]) /\ reads' = reads + 1 /\ op' = "return"
              /\ UNCHANGED <<boot, seeded, published, cur, selected, authority, frames,
                  lookupBinding, workBinding, lookupChanged, workChanged, alive, parked, paused,
                  effects, baseReads, metaEntries, lookupEffects, nativeLive, foreignReturns,
                  nativeAnswers, callbackDone, duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
ResumeLookup == /\ op = "return" /\ cur.k > 0 /\ Top.kind = "lookup"
                /\ op' = "enter" /\ frames' = Pop
                /\ cur' = IF Bug = "lose-request" /\ Top.request.s = "work"
                           THEN [Top.request EXCEPT !.args = <<>>] ELSE Top.request
                /\ authority' = value
                /\ selected' = IF Bug = "reselect" THEN Raw(Vtable(Top.request.r), Top.request.s) ELSE value
                /\ UNCHANGED <<boot, seeded, published, value, lookupBinding, workBinding,
                    lookupChanged, workChanged, alive, parked, paused, effects, reads, baseReads,
                    metaEntries, lookupEffects, nativeLive, foreignReturns, nativeAnswers,
                    callbackDone, duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
EnterNative == /\ op = "enter" /\ selected \in {"native0", "native1"} /\ ~parked
               /\ nativeLive' = 1 /\ op' = IF NativeMode = "sync" THEN "send" ELSE "native-detach"
               /\ frames' = IF Bug = "native-tail" THEN frames ELSE Append(frames, Save("native", cur, selected))
               /\ cur' = Request(0, "helper", <<>>, IF Bug = "native-tail" THEN cur.k ELSE Len(frames)+1)
               /\ UNCHANGED <<boot, seeded, published, selected, authority, value, lookupBinding,
                   workBinding, lookupChanged, workChanged, alive, parked, paused, effects, reads,
                   baseReads, metaEntries, lookupEffects, foreignReturns, nativeAnswers, callbackDone,
                   duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
DetachNative == /\ op = "native-detach" /\ op' = "send" /\ nativeLive' = 0 /\ foreignReturns' = foreignReturns+1
                /\ UNCHANGED <<boot, seeded, published, cur, selected, authority, value, frames,
                    lookupBinding, workBinding, lookupChanged, workChanged, alive, parked, paused,
                    effects, reads, baseReads, metaEntries, lookupEffects, nativeAnswers, callbackDone,
                    duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
EnterHelper == /\ op = "enter" /\ selected = "helperBody" /\ ~parked
               /\ op' = "return" /\ value' = "answer"
               /\ UNCHANGED <<boot, seeded, published, cur, selected, authority, frames,
                   lookupBinding, workBinding, lookupChanged, workChanged, alive, parked, paused,
                   effects, reads, baseReads, metaEntries, lookupEffects, nativeLive, foreignReturns,
                   nativeAnswers, callbackDone, duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
ResumeNative == /\ op = "return" /\ cur.k > 0 /\ Top.kind = "native"
                /\ cur' = Top.request /\ frames' = Pop /\ callbackDone' = TRUE
                /\ nativeLive' = 0 /\ nativeAnswers' = nativeAnswers+1
                /\ foreignReturns' = foreignReturns + (IF NativeMode = "sync" THEN 1 ELSE 0)
                /\ UNCHANGED <<boot, seeded, published, op, selected, authority, value, lookupBinding,
                    workBinding, lookupChanged, workChanged, alive, parked, paused, effects, reads,
                    baseReads, metaEntries, lookupEffects, duplicateSeen, drivers, overlapSeen,
                    denied, done, finalK, selectedLookups>>
Finish == /\ op = "return" /\ cur.k = 0 /\ done' = TRUE /\ op' = "done"
          /\ finalK' = cur.k /\ drivers' = 0
          /\ UNCHANGED <<boot, seeded, published, cur, selected, authority, value, frames,
              lookupBinding, workBinding, lookupChanged, workChanged, alive, parked, paused,
              effects, reads, baseReads, metaEntries, lookupEffects, nativeLive, foreignReturns,
              nativeAnswers, callbackDone, duplicateSeen, overlapSeen, denied, selectedLookups>>
Pause == /\ op = "lookup-read" /\ ~parked /\ ~paused
         /\ paused' = TRUE
         /\ parked' = (nativeLive = 0 \/ Bug = "native-escape")
         /\ denied' = (denied \/ nativeLive > 0)
         /\ UNCHANGED <<boot, seeded, published, op, cur, selected, authority, value, frames,
             lookupBinding, workBinding, lookupChanged, workChanged, alive, effects, reads,
             baseReads, metaEntries, lookupEffects, nativeLive, foreignReturns, nativeAnswers,
             callbackDone, duplicateSeen, drivers, overlapSeen, done, finalK, selectedLookups>>
Wake == /\ parked /\ parked' = FALSE
        /\ effects' = effects + (IF Bug = "lookup-replay" THEN 1 ELSE 0)
        /\ UNCHANGED <<boot, seeded, published, op, cur, selected, authority, value, frames,
            lookupBinding, workBinding, lookupChanged, workChanged, alive, paused, reads,
            baseReads, metaEntries, lookupEffects, nativeLive, foreignReturns, nativeAnswers,
            callbackDone, duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
ReplaceLookup == \E level \in 1..Depth :
                 /\ published /\ level \notin lookupChanged /\ lookupChanged' = lookupChanged \cup {level}
                 /\ lookupBinding' = [lookupBinding EXCEPT ![level] = "lookup1"]
                 /\ UNCHANGED <<boot, seeded, published, op, cur, selected, authority, value, frames,
                     workBinding, workChanged, alive, parked, paused, effects, reads, baseReads,
                     metaEntries, lookupEffects, nativeLive, foreignReturns, nativeAnswers, callbackDone,
                     duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
ReplaceWork == /\ published /\ ~workChanged /\ workChanged' = TRUE /\ workBinding' = "native1"
               /\ UNCHANGED <<boot, seeded, published, op, cur, selected, authority, value, frames,
                   lookupBinding, lookupChanged, alive, parked, paused, effects, reads, baseReads,
                   metaEntries, lookupEffects, nativeLive, foreignReturns, nativeAnswers, callbackDone,
                   duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
Held == ({selected, value} \cap Methods) \cup ({frames[i].owner : i \in 1..Len(frames)} \ {None})
Collect == /\ alive' = alive \cap ({workBinding, "lookup1", "native1", "helperBody"} \cup {lookupBinding[level] : level \in 1..Depth}
                 \cup IF Bug = "drop-lookup-code" THEN {value} \cap Methods ELSE Held)
           /\ UNCHANGED <<boot, seeded, published, op, cur, selected, authority, value, frames,
               lookupBinding, workBinding, lookupChanged, workChanged, parked, paused, effects,
               reads, baseReads, metaEntries, lookupEffects, nativeLive, foreignReturns, nativeAnswers,
               callbackDone, duplicateSeen, drivers, overlapSeen, denied, done, finalK, selectedLookups>>
Duplicate == /\ callbackDone /\ ~duplicateSeen /\ duplicateSeen' = TRUE
             /\ nativeAnswers' = nativeAnswers + (IF Bug = "double-completion" THEN 1 ELSE 0)
             /\ UNCHANGED <<boot, seeded, published, op, cur, selected, authority, value, frames,
                 lookupBinding, workBinding, lookupChanged, workChanged, alive, parked, paused,
                 effects, reads, baseReads, metaEntries, lookupEffects, nativeLive, foreignReturns, callbackDone,
                 drivers, overlapSeen, denied, done, finalK, selectedLookups>>
Reenter == /\ nativeLive > 0 /\ ~overlapSeen /\ overlapSeen' = TRUE
           /\ drivers' = drivers + (IF Bug = "overlap" THEN 1 ELSE 0)
           /\ UNCHANGED <<boot, seeded, published, op, cur, selected, authority, value, frames,
               lookupBinding, workBinding, lookupChanged, workChanged, alive, parked, paused,
               effects, reads, baseReads, metaEntries, lookupEffects, nativeLive, foreignReturns,
               nativeAnswers, callbackDone, duplicateSeen, denied, done, finalK, selectedLookups>>
Drive == Start \/ Send \/ EnterLookup \/ CallLookupHelper \/ ResumeLookupHelper \/ LookupRead \/ ResumeLookup \/ EnterNative \/ DetachNative \/ EnterHelper \/ ResumeNative \/ Finish
Next == Bootstrap \/ Drive \/ Pause \/ Wake \/ ReplaceLookup \/ ReplaceWork \/ Collect \/ Duplicate \/ Reenter
Spec == Init /\ [][Next]_vars /\ WF_vars(Bootstrap) /\ WF_vars(Drive) /\ WF_vars(Wake)

BootstrapReady == published => seeded /\ boot = 3
SelectionPreserved == op = "enter" => selected = authority
RequestPreserved == op = "enter" /\ cur.s = "work" => cur = RootRequest
SelectedCallable == op = "enter" => selected \in Methods
ResolutionBound == Len(frames) <= Depth+2
NoLookupReplay == effects = reads + (IF op \in {"lookup-read", "lookup-callback"} THEN 1 ELSE 0)
                  + Cardinality({i \in 1..Len(frames) : frames[i].kind = "lookup-resume"})
DirectEntry == selectedLookups = 0
CodeAlive == Held \subseteq alive
NativeBarrier == parked => nativeLive = 0
SingleDriver == drivers <= 1
Once == nativeAnswers <= 1 /\ foreignReturns <= 1
CompletedCorrectly == done => /\ value = "answer" /\ finalK = 0 /\ nativeLive = 0
                              /\ Len(frames) = 0 /\ nativeAnswers = 1 /\ foreignReturns = 1
                              /\ metaEntries = 1..Depth
Progress == <>done
NoCompletedSend == ~done
NoLookupCallback == ~\E i \in 1..Len(frames) : frames[i].kind = "lookup-resume"
NoSuspendedLookup == ~parked
NoSuspendedNativeCallback == ~(parked /\ NativeMode = "async" /\ foreignReturns = 1)
NoDeniedNativeSuspend == ~denied
NoReplacedLookupEntry == "lookup1" \notin lookupEffects
NoStaleCompletion == ~duplicateSeen
=============================================================================

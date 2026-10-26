---------------------------- MODULE NativeScopes ----------------------------
EXTENDS Naturals, Sequences
CONSTANT Bug
ASSUME Bug \in {"none", "clear-ancestors", "foreign-return", "ignore-owner", "reuse-ticket", "ignore-barrier"}
VARIABLE q
vars == <<q>>
Init == q = [phase |-> 0, frames |-> <<>>, outerLive |-> FALSE, innerLive |-> FALSE,
             parked |-> FALSE, foreignTried |-> FALSE, wrongTried |-> FALSE,
             duplicateTried |-> FALSE, used |-> FALSE, answers |-> 0, wrongAccepted |-> FALSE]
PushOuter == /\ q.phase = 0
             /\ q' = [q EXCEPT !.phase = 1, !.frames = <<"outer">>, !.outerLive = TRUE]
PushInner == /\ q.phase = 1
             /\ q' = [q EXCEPT !.phase = 2, !.frames = Append(@, "inner"), !.innerLive = TRUE]
ForeignReturn == /\ q.phase = 2 /\ ~q.foreignTried
                 /\ q' = [q EXCEPT !.foreignTried = TRUE,
                           !.frames = IF Bug = "foreign-return" THEN <<"outer">> ELSE @]
InnerReturn == /\ q.phase = 2
               /\ q' = [q EXCEPT !.phase = 3, !.innerLive = FALSE,
                         !.frames = IF Bug = "clear-ancestors" THEN <<>> ELSE SubSeq(@, 1, Len(@)-1)]
AttemptSuspend == /\ q.phase = 3
                  /\ q' = [q EXCEPT !.phase = 4, !.parked = (Len(q.frames) = 0 \/ Bug = "ignore-barrier")]
\* The callback handles refusal and finishes synchronously. Only then can its
\* outer native caller return; an asynchronous child did not erase that caller.
OuterReturn == /\ q.phase = 4 /\ ~q.parked
               /\ q' = [q EXCEPT !.phase = 5, !.outerLive = FALSE, !.frames = <<>>]
DetachedWait == /\ q.phase = 5
                /\ q' = [q EXCEPT !.phase = 6, !.parked = TRUE]
Allowed(caller) == (caller = "owner" \/ Bug = "ignore-owner") /\ (~q.used \/ Bug = "reuse-ticket")
WrongCompletion == /\ q.phase = 6 /\ ~q.wrongTried
                   /\ q' = [q EXCEPT !.wrongTried = TRUE,
                             !.wrongAccepted = Allowed("other"),
                             !.used = IF Allowed("other") THEN TRUE ELSE @,
                             !.answers = @ + (IF Allowed("other") THEN 1 ELSE 0)]
Complete == /\ q.phase = 6 /\ Allowed("owner")
            /\ q' = [q EXCEPT !.phase = 7, !.parked = FALSE, !.used = TRUE, !.answers = @+1]
Duplicate == /\ q.phase = 7 /\ ~q.duplicateTried
             /\ q' = [q EXCEPT !.duplicateTried = TRUE,
                       !.answers = @ + (IF Allowed("owner") THEN 1 ELSE 0)]
Advance == PushOuter \/ PushInner \/ InnerReturn \/ AttemptSuspend \/ OuterReturn \/ DetachedWait \/ Complete
Next == Advance \/ ForeignReturn \/ WrongCompletion \/ Duplicate
Spec == Init /\ [][Next]_vars /\ WF_vars(Advance)
NativeFrames == /\ q.outerLive = ("outer" \in {q.frames[i] : i \in 1..Len(q.frames)})
                /\ q.innerLive = ("inner" \in {q.frames[i] : i \in 1..Len(q.frames)})
NoEscape == q.parked => ~q.outerLive /\ ~q.innerLive
OwnerOnly == ~q.wrongAccepted
AtMostOnce == q.answers <= 1
Progress == <>(q.phase = 7 /\ q.answers = 1)
NoDetachedWait == ~(q.phase = 6 /\ q.parked)
NoRefusal == q.phase < 4
=============================================================================

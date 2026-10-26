------------------------- MODULE ContinuationSend -------------------------
EXTENDS Naturals, FiniteSets, Sequences, TLC
CONSTANTS Tasks, Methods, First, Second, Calls, Bug
ASSUME /\ First \in Methods /\ Second \in Methods /\ First # Second
       /\ Calls > 0
       /\ Bug \in {"none", "relookup", "replay", "tail-push", "cross-answer",
                    "drop-code", "cached-lookup", "double-wake", "native-lookup", "unfair"}
VARIABLES task, binding, changed, alive, ready, waiting, used
vars == <<task, binding, changed, alive, ready, waiting, used>>
PCs == {"lookup", "lookup-effect", "select", "selected", "body", "answer", "done"}
Key(t) == <<t, task[t].round, task[t].pc>>
Init == /\ task = [t \in Tasks |->
          [pc |-> "lookup", round |-> 1, effects |-> 0, entries |-> 0,
           answers |-> 0, selected |-> First, observed |-> First, entered |-> First,
           destination |-> t, depth |-> 1, wakes |-> 0, pauses |-> 0]]
        /\ binding = First /\ changed = FALSE /\ alive = Methods
        /\ ready = Tasks /\ waiting = {} /\ used = {}

\* Execution owns task records. Id sees Oop identities and the explicit
\* transfer request. In particular, it does not decode a continuation payload.
\* Each action below is an atomic logical transition, not a native call stack.
Lookup(t) == /\ t \in ready /\ task[t].pc = "lookup"
             /\ task' = [task EXCEPT ![t].pc = "lookup-effect",
                                    ![t].effects = @ + 1]
             /\ UNCHANGED <<binding, changed, alive, ready, waiting, used>>
LookupWork(t) == /\ t \in ready /\ task[t].pc = "lookup-effect"
                 /\ task' = [task EXCEPT ![t].pc = "select"]
                 /\ UNCHANGED <<binding, changed, alive, ready, waiting, used>>
Select(t) == /\ t \in ready /\ task[t].pc = "select"
             /\ task' = [task EXCEPT ![t].pc = "selected", ![t].observed = binding,
                 ![t].selected = IF Bug = "cached-lookup" THEN First ELSE binding]
             /\ UNCHANGED <<binding, changed, alive, ready, waiting, used>>
Enter(t) == /\ t \in ready /\ task[t].pc = "selected"
            /\ task' = [task EXCEPT ![t].pc = "body", ![t].entries = @ + 1,
                 ![t].entered = IF Bug = "relookup" THEN binding ELSE task[t].selected,
                 ![t].effects = @ + (IF Bug = "replay" THEN 1 ELSE 0)]
            /\ UNCHANGED <<binding, changed, alive, ready, waiting, used>>
Body(t) == /\ t \in ready /\ task[t].pc = "body"
           /\ task' = [task EXCEPT ![t].pc = "answer"]
           /\ UNCHANGED <<binding, changed, alive, ready, waiting, used>>
Answer(t) == /\ t \in ready /\ task[t].pc = "answer"
             /\ task' = [task EXCEPT ![t].answers = @ + 1,
                 ![t].pc = IF task[t].round = Calls THEN "done" ELSE "lookup",
                 ![t].round = IF task[t].round = Calls THEN @ ELSE @ + 1,
                 ![t].destination = IF Bug = "cross-answer"
                     THEN CHOOSE other \in Tasks : other # t ELSE @,
                 ![t].depth = @ + (IF Bug = "tail-push" /\ task[t].round < Calls THEN 1 ELSE 0)]
             /\ ready' = IF task[t].round = Calls THEN ready \ {t} ELSE ready
             /\ UNCHANGED <<binding, changed, alive, waiting, used>>
\* A lookup may park after an effect, or after selection but before entry.
\* Body suspension uses the identical scheduler handoff. One park per key is
\* a finite exploration bound, not a proposed language restriction.
Pause(t) == /\ t \in ready /\ task[t].pc \in {"lookup-effect", "selected", "body"}
            /\ Key(t) \notin used
            /\ task' = [task EXCEPT ![t].pauses = @ + 1]
            /\ ready' = ready \ {t} /\ waiting' = waiting \cup {t}
            /\ UNCHANGED <<binding, changed, alive, used>>
Wake(t) == /\ t \in waiting
           /\ ~(Bug = "native-lookup" /\ task[t].pc \in {"lookup-effect", "selected"})
           /\ task' = [task EXCEPT ![t].wakes = @ + 1]
           /\ ready' = ready \cup {t} /\ waiting' = waiting \ {t}
           /\ used' = used \cup {Key(t)}
           /\ UNCHANGED <<binding, changed, alive>>
StaleWake(t) == /\ Bug = "double-wake" /\ t \in ready /\ Key(t) \in used
                /\ task' = [task EXCEPT ![t].wakes = @ + 1]
                /\ UNCHANGED <<binding, changed, alive, ready, waiting, used>>
Replace == /\ ~changed /\ binding' = Second /\ changed' = TRUE
           /\ UNCHANGED <<task, alive, ready, waiting, used>>
Held == {task[t].selected : t \in {t \in Tasks : task[t].pc \in {"selected", "body", "answer"}}}
Collect == /\ alive' = alive \cap ({binding, Second} \cup IF Bug = "drop-code" THEN {} ELSE Held)
           /\ UNCHANGED <<task, binding, changed, ready, waiting, used>>
Step(t) == Lookup(t) \/ LookupWork(t) \/ Select(t) \/ Enter(t) \/ Body(t) \/ Answer(t)
Next == (\E t \in Tasks : Step(t) \/ Pause(t) \/ Wake(t) \/ StaleWake(t)) \/ Replace \/ Collect
Spec == Init /\ [][Next]_vars
        /\ IF Bug = "unfair" THEN TRUE
           ELSE \A t \in Tasks : WF_vars(Step(t)) /\ WF_vars(Wake(t))

TypeOK == /\ binding \in Methods /\ alive \subseteq Methods
          /\ ready \subseteq Tasks /\ waiting \subseteq Tasks
          /\ \A t \in Tasks : task[t].pc \in PCs /\ task[t].round \in 1..Calls
SingleOwner == /\ ready \cap waiting = {}
               /\ ready \cup waiting = {t \in Tasks : task[t].pc # "done"}
LookupCurrent == \A t \in Tasks : task[t].pc \in {"selected", "body", "answer"} => task[t].selected = task[t].observed
SelectedEntry == \A t \in Tasks : task[t].pc \in {"body", "answer"} => task[t].entered = task[t].selected
NoReplay == \A t \in Tasks : task[t].effects = task[t].answers + (IF task[t].pc \in {"lookup", "done"} THEN 0 ELSE 1)
ExactEntries == \A t \in Tasks : task[t].entries = task[t].answers + (IF task[t].pc \in {"body", "answer"} THEN 1 ELSE 0)
AnswerDestination == \A t \in Tasks : task[t].destination = t
TailBound == \A t \in Tasks : task[t].depth = 1
CodeAlive == Held \subseteq alive
OneShotWake == \A t \in Tasks : task[t].wakes <= task[t].pauses
Completed == \A t \in Tasks : task[t].pc = "done" /\ task[t].answers = Calls
Progress == <>Completed
NoLookupPause == ~\E t \in waiting : task[t].pc = "lookup-effect"
NoSelectedReplacement == ~\E t \in waiting : task[t].pc = "selected" /\ task[t].selected # binding
NoBodyPause == ~\E t \in waiting : task[t].pc = "body"
NoTwoWaiting == Cardinality(waiting) < 2
==========================================================================

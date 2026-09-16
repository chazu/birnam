# 80A — Concurrency API sketches

> **Status: illustrative proposals, 2026-09-16.** These programs explore
> [Design 80](80-concurrency-domains-and-structured-tasks.md). They are not
> executable examples of a shipped API. Selector names and lifecycle details
> remain proposals. Existing Birnam blocks, sends, Maps, and Cells supply the
> syntax; Tasks, domains, Mutex, Duration, and the HTTP client below describe
> proposed facilities.
> Updated after the results/lifecycle discussion: `await` and `outcome` both wait
> for settlement including cleanup; automatic scope joining replaces the earlier
> standalone `join` sketch. Design 80 section 7 records the failure and debugger
> policy, including remaining semantic questions.
> **Post-review scope clarification:** worker code may be updated by stopping
> and recreating the domain. No worker hot-reload or automatic cross-domain
> propagation is required. Application/native global-state safety belongs to
> the user; the sketches imply no library certification or automatic locking.
> [80H](80h-concurrency-review-edge-cases.md) adds explicit Error-value/failure
> examples and four edge-case traces for the second review.

## 1. Two independent I/O operations

```text
let page := [Tasks scope: [:scope |
  let profile := [scope fork: [| [Http get: "/profile"]]];
  let activity := [scope fork: [| [Http get: "/activity"]]];

  {#profile -> [profile await],
   #activity -> [activity await]}
]];

[System println: [page printString]]
```

Both tasks are created before either result is awaited. Waiting for `profile`
does not stop `activity` from progressing. The HTTP adapter suspends a waiting
task instead of blocking the domain. Source code keeps ordinary sequential
evaluation and message sends.

`scope:` runs its body in a task-capable context and answers the body's value
after its children and required cleanup settle on successful exit. The proposed
`fork:` returns a child-task handle promptly rather than running the child to
completion first. Both tasks belong to this scope and share its domain.

These are success-path examples. `await` returns a successful value or propagates
task failure/cancellation. `outcome` instead returns inspectable terminal state;
both wait for children and required cleanup. Ordinary returned Error values
remain values. Merely placing a returned Error in the map must not silently
become an exception or trigger sibling cancellation. The precise outcome objects
and task-control implementation remain open.

## 2. CPU work on two cores

Assume `render.image` contains a named `ImageWorker` object with a `render:`
method accepting a filename and returning a transferable value. The image's
behavior is installed in each domain; no caller closure is sent to a worker.

```text
let previews := [Tasks scope: [:scope |
  let left := [scope domainFrom: "render.image"];
  let right := [scope domainFrom: "render.image"];

  let a := [left submit: #ImageWorker
    selector: #render: argument: "a.png"];
  let b := [right submit: #ImageWorker
    selector: #render: argument: "b.png"];

  {#a -> [a await], #b -> [b await]}
]]
```

The domains can execute simultaneously on separate cores. Each owns its own
object environment. `argument:` crosses the copied-value boundary explicitly;
the result must also be transferable. Local `fork:` alone would not spread work
over cores.

For this proposed convenience API, `domainFrom:` creates a domain owned by the
scope. Submitted operations are owned through that domain; successful scope
exit settles the operations and shuts down its owned domains. This establishes
an ownership rule rather than relying on whichever task happens to call
`submit:`. Long-lived borrowed domains and pools need separate ownership APIs.

Each handle initially addresses one default serial dispatcher. Each accepted
request executes in its own task/scope; the dispatcher waits for its settled
outcome before starting the next. Request failure/cancellation is isolated from
the service loop under its explicit Birnam service policy, while ordinary scope
rules apply inside a request. The caller's owning scope can still cancel the
whole domain. Submitting to and awaiting one's own serial queue can deadlock.

Domain startup and bounded queue admission can suspend the caller. Submission
does not await execution completion, so both requests can be outstanding before
either `await`. The first request may finish before the second is admitted;
overlap is permitted rather than guaranteed for every job size.

A production service would normally reuse domains across many jobs. Two domains
per batch here make parallel ownership visible without introducing pool policy.
Process and thread placement must expose their different failure containment.
To update the worker's code, create an updated image, stop the old domain, and
start a new one. Existing worker domains do not inherit edits made in the main
domain. The user coordinates the application's code and protocol compatibility.

## 3. Shared mutable state inside one domain

```text
let count := [0 asCell];
let lock := [Mutex new];
let increment := [|
  [lock during: [| [count set: [[count value] + 1]]]]
];

[Tasks scope: [:scope |
  [scope fork: increment];
  [scope fork: increment]
]];

[System println: [count value]]
```

The intended successful result is `2`. Both tasks capture the same Cell and
Mutex. The scope joins both before the final read. Without synchronization, an
interleaving could lose one increment even though only one task executes at a
time in this domain.

`during:` is a tentative selector for the accepted protected-section direction:
a contending task may suspend before entry; inside, cancellation is deferred and
waiting is prohibited. The holder releases the lock and restores the enclosing
cancellation state when its block finishes or unwinds. Automatic
timeslicing remains enabled; other runnable tasks may proceed. The lock protects
only code cooperating through this Mutex and does not freeze the object graph,
prevent reflection, or turn ordinary methods into atomic operations.

The selector and implementation are not selected. Reentrancy, lock ordering,
cancellation during acquisition/release, and enforcement of the no-wait rule
still need decisions. A prohibited wait must not implicitly release the lock,
and unlocking does not roll back partial mutation.

## 4. A bounded wait, with explicit cancellation

This fragment runs inside a scope and assumes `job` is one of its child tasks:

```text
[job awaitFor: [Duration milliseconds: 500] ifTimedOut: [|
  [System println: "Still running; requesting cancellation"];
  [job cancel];
  [job outcome]
]]
```

`awaitFor:ifTimedOut:` waits for a result up to the duration, or runs the timeout
block in the caller. Timing out alone does not cancel the task. `cancel` requests
cancellation; `outcome` waits for termination and required cleanup. The fragment can
therefore take longer than 500 milliseconds. A stuck native call can prevent
joining from completing; this example promises no hard completion deadline.

The proposed successful `await` returns the task body's value and otherwise
propagates task failure/cancellation. `outcome` observes settlement without that
propagation. Merely observing an outcome does not suppress an ordinary scope's
failure policy. Deliberate child cancellation alone does not fail an otherwise
successful scope; a genuine cleanup or sibling failure still can. The observer's
own cancellation can also interrupt its `outcome` wait. Exact result types and
the observation-timeout race remain design work; final task publication follows
Design 80's agreed cancellation rule. Neither waiting operation retries work or
claims to undo external effects.

## 5. What the sketches let us evaluate

- `fork:` shares a domain; `submit:selector:argument:` crosses one. The difference
  matters because captured Cells and ordinary object identities stay local.
- Scopes own work. Synchronization protects shared mutation. These are separate
  responsibilities, and leaving a scope does not retrospectively make its child
  operations atomic.
- Ordinary methods and Blocks remain the authoring surface. An adapter must
  provide real task suspension; message-send syntax does not make a blocking
  foreign call asynchronous.
- Method lookup remains ordinary local lookup. Main-domain edits do not alter a
  worker's code environment; stop and recreate the worker to update it. No new
  in-place worker redefinition or live-upgrade compatibility layer is required.
- Waiting APIs need precise result and settlement contracts before choosing
  shorthand. In particular, cancellation must not be smuggled into the meaning
  of a plain timeout.

The next review should judge whether these distinctions are understandable in
the small programs, then settle the remaining outcome, debugger, and shared-state
protection contracts.
Do not infer implementation approval from a preference for a selector spelling.

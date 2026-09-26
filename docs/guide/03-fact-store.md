# The fact store

Under every birnam program sits `db` — a historied store of
**entity–attribute–value facts**. Cells refer to an entity and attribute in that
store; globals, application state and metadata use the same fact model.

During Design 90, the [native Store and Cell protocol](../../rust/birnam-store/README.md)
is available through Bootstrap, including compiled source, history and retention.
Globals retains its `__globals__` facts. [Relational queries](../../rust/birnam-relational/README.md)
also run through Id messages. Compiled top-level bindings now write Store-backed
Globals and are visible to fact queries and history. Remaining self-hosted
conveniences, full CLI and image startup are not yet restored.

## Asserting facts

```
let alice := [db entity];                       // a fresh entity
[db assert: alice attr: #name value: "Alice"];
[db assert: alice attr: #city value: #Boston];
[db assert: alice attr: #age  value: 33];
```

Any value can sit in the value slot — symbols, strings, numbers, other
entities. For a cardinality-one attribute, re-assertion supersedes the previous
value. Retaining its past values requires opting the attribute into history and
keeping the corresponding retention window. Re-asserting the *same* value is an
idempotent no-op.

The object-side view of the same store is the **cell**:

```
let c := [db cellFor: alice attr: #age];
[c set: 34];
[c value]                                        // 34
```

## Queries: findAll:where:

`findAll:where:` is a datalog query: logic variables start with `?`, and the
`where:` vector holds clause vectors of shape `#[entity attribute value]`.
Constants in any position match by **value**; shared variables join.

```
[db findAll: ?p where: #[ #[?p #city #Boston] ]];            // who is in Boston
[db findAll: ?c where: #[ #[?p #name "Alice"] #[?p #city ?c] ]];  // Alice's city — a join
```

The answer is a Vector of the distinct bindings of the asked variable.

## Predicates, negation, disjunction

A clause whose attribute is a comparison symbol filters a bound variable:

```
[db findAll: ?p where: #[ #[?p #age ?a] #[?a #gt 18] ]];     // adults
[db findAll: ?a where: #[ #[?p #age ?a] #[?a #>= 30] ]];     // #eq #ne #lt #gt #<= #>= …
```

`#not` wraps a clause; `#or` unions clauses:

```
[db findAll: ?a where: #[ #[?p #age ?a] #[#not #[?a #eq 40]] ]];
[db findAll: ?a where: #[ #[?p #age ?a] #[#or #[?a #lt 20] #[?a #gt 35]] ]];
```

## Aggregates and grouping

```
[db count: ?p where: #[ #[?p #age ?a] ]];
[db sum: ?a   where: #[ #[?p #age ?a] ]];       // also min: max: avg:
[db groupBy: ?t aggregate: #count of: ?p where: #[ #[?p #team ?t] ]]
  // -> a Map of team -> member count
```

## Rules

`defineRule:params:body:` names a derived relation. Multiple definitions of
one name are **disjunction**, and a rule may reference itself — so a base case
plus a recursive case is two calls:

```
[db defineRule: #ancestor params: #[?x ?y] body: #[ #[?x #parent ?y] ]];
[db defineRule: #ancestor params: #[?x ?y]
    body: #[ #[?x #parent ?z] #[#rule #ancestor ?z ?y] ]];

[db findAll: ?d where: #[ #[#rule #ancestor tom ?d] ]]       // all descendants
```

A rule is queried with a `#[#rule #name args…]` clause, usable anywhere an
ordinary clause is.

## Time travel

Every assertion advances the store's transaction counter. `basis` captures a
moment; `asOf:` reads the world as it was; `since:` reads only what changed.

```
let earlier := [db basis];
// … more assertions …
[db findAll: ?p where: #[ #[?p #city #Boston] ] asOf: earlier];
[db findAll: ?p where: #[ #[?p #city #Boston] ] since: earlier];
[db valueOf: #account attr: #balance asOf: earlier]
```

New entities and facts are always visible to as-of; *prior values* of a
re-asserted attribute are visible only if that attribute opted into history.

## Pull: entities as records

`pull:spec:` materializes an entity into a Map. A spec element is a leaf
attribute, or a nested `#[ #refAttr subspec ]` that follows a reference one
level down. Cardinality is itself a fact — declare an attribute `:many` and
its pulls answer a Vector:

```
[db assert: #pet attr: #cardinality value: #many];
[db pull: alice spec: #[ #name #pet ]];
[db pull: bob   spec: #[ #name #[#friend #[ #name #city ]] ]]
```

## The system is in the store

The reflective spine uses the same facts you do: the sealed current
`DispatchGeneration` owns executable source authority, `#__modsrc__` mirrors
loaded module text, `#__methodops__` preserves ordered source-backed method
operations from ordinary scripts and the REPL, `#__globals__` holds the
namespace, and the doc registry holds docstrings. `#__methods__` is retained as
legacy/base method-source storage rather than the complete current source
authority. `[Doc undocumentedSelectors]`, hover in the LSP, and this reference
are all just queries.

The compatibility records support reflection and explicit image recovery; they
do not override live dispatch. Image compilation and `--rebuild` consume the
generation's sealed module source map followed by its ordered method operations.
The later file-backed evolution policy that was planned around that authority
was withdrawn.

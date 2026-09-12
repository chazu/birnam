# Optional native field projection declaration, v1

This optional module export declares the meaning of an exact native Method:

```c
unsigned int birnam_native_field_projection_v1(const void *method, void *nil);
```

The query is synchronous, allocation-free and effect-free. Zero declines; codes
1 through 128 declare a zero-argument Method returning the existing Oop in
receiver payload word `code - 1`. The supported domain is a non-null, live,
properly aligned receiver with sufficient all-Oop payload storage. Within that
domain the Method has no other behavior: no writes, sends, allocation, lookup,
registration, collection, instrumentation or identity-dependent branch. It must
not depend on a presumed receiver class name. Unsupported invocation shapes
continue to use the actual Method.

The producer verifies that it has initialized against the supplied floor's
canonical nil before declaring any Method. Any API calls or mutable state used
by a proposed implementation must be absent from the successful projection
path, or independently verified by this query as having no effects. Producers
must decline instrumented or otherwise changed implementations. A consumer
requeries a retained declaration before using its field index.

This is an unsafe native-module contract, like a Method or trace hook contract;
ordinary Id metadata cannot attach this assertion to opaque native code. The
runtime obtains the declaration only from the already-resident image containing
the actual resolved Method, verifies ownership rather than accepting a dependency
symbol, and pins that image while using the fact. Missing/unknown declarations
mean ordinary execution. Current method bindings and storage eligibility remain
separate runtime checks; selector spelling supplies no authority.

Relational's existing LogicVar name projection supplies the first production
declaration. Its named-variable factory is cached and is not declared as a pure
allocation; Store attributes and Cell operations are not payload projections.

The declaration adds no IdApi fields or Method parameters and changes no
canonical descriptor or image format. Compiler values, slot maps and recipes
remain data that an Id implementation can produce under the same admission rules.

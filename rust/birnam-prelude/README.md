# Prelude

Prelude constructs System through Oop-only Id, Namespace, ObjectModel and Roots
messages. It requires completed Roots, ObjectVt and ObjectModel bindings. A new
System has a delegated vtable and zero payload; Prelude publishes SystemVt and
System and assigns the vtable's owner. An existing System binding is reused.

The module caches its completed result for the initial Namespace. Repeated loads
return that same System even after namespace rebinding, so the Namespace and
System remain rooted. This is completed module state, not a table of future
services. Reentrant construction is rejected. Temporary construction roots are
released after publication; native identity/borrow compatibility exports are gone.

`bash scripts/oop-id-slice-test.sh` checks construction, zero payload, ownership,
and repeated loading after rebinding and collection. Full Bootstrap, compiler,
execution, image and worker integration remain pending.

# Host support

Local Rust helpers for scoped Id sends, copied value projection, creating-thread
identity, bounded file input and owned command transport DTOs. This is an rlib,
not a native module or language owner. It owns no loader, runtime, scheduler or
foreign object representation. Native Host owners and the executable share Id's
actual Namespace and Roots; each holds its own local scope tokens.

Native owner initialization, receiver/argument retention, creating-thread checks
and Error construction are implemented here as local code in each dylib. Selector
lists and behavior remain with their respective native owners. The executable
uses the same value/root helpers but does not link owner implementations.

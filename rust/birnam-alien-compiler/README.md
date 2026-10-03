# Alien wrapper compiler over Id

`AlienCompiler` is a separately loaded native module. It owns the existing
signature grammar, deterministic scalar/aggregate ABI classification and
wrapper/callback C generation. It owns no foreign address, library, callback,
executable mapping or caller state. TCC remains the platform ABI authority for
layout, alignment and calling convention.

The constructor receives completed Namespace and publishes `AlienCompiler` and
`AlienCompilerVt`. `compile:signature:` accepts a function-name String (nil for a
callback) and a signature String; it returns generated C as a String or the
existing `#badSignature` Error. Inputs are retained and byte borrows copied;
creating-thread calls and repeated construction follow the existing native-owner
rules. No Rust Signature or private compiler state crosses the native boundary.

Alien resolves the foreign symbol before this request, preserving observation
order, then uses TCC to compile/load the wrapper. Saved wrapper restoration maps
recorded object bytes without calling the compiler. Parser, ABI and emission
algorithms and generated C prelude were moved unchanged.

Validation: `cargo test -p birnam-alien-compiler` covers parsing, classification
and emission. Installed tests cover scalar/aggregate calls, callbacks, collection
and reclamation; `scripts/alien-image-test.py PREFIX` covers source-free mapped,
rebuilt and baked wrapper recovery. The installed `native_compilers` Host test
checks collecting/replaced owners and exact native failure propagation.

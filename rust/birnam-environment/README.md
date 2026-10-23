# Environment lookup

The v2 initializer installs `System getEnv:` using an Oop Namespace context.
Names must have the captured exact String vtable. Their current `bytes` provider
supplies byte Oops in one `apply:to:with:` call; no borrowed pointer crosses Id.
UTF-8 names use the existing environment lookup. Missing values return language
nil, empty values produce empty Strings, and invalid Unicode produces `ioError`.
There is no environment mutation API.

String construction remains live. Error lookup happens after diagnostic String
construction, so a factory published during that callback is visible. Absence of
an Error factory returns language nil; absence of a diagnostic String is invalid.
One private root Sequence retains completed dependencies and helper Methods.
Id call roots protect incoming values and diagnostic receivers. Reload preserves
current public bindings, including removal.

`bash scripts/oop-id-slice-test.sh` uses isolated, explicitly supplied environment
variables to test Unicode/binary/empty/missing values, exact String recognition,
live byte and String methods, lazy Error publication, malformed results, captured
Methods and callback GC. Full-language integration remains pending.

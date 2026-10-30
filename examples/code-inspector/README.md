# Code inspector

An unattended application exercising owner-based reflection and explicit
relational code description. Run against a rebuilt prefix:

```sh
python3 scripts/code-inspector.py /path/to/prefix
python3 scripts/code-inspector-test.py
```

It creates a parent/child object pair, defines an override, replaces it and adds
a second method, then removes the override to expose the parent's method. At each
stage it checks direct versus inherited inventory, vtable ownership, current
behavior and two global aliases. Additional checks cover bound nil, missing and
unnamed bindings, Namespace versus Globals precedence, and Doc reverse naming.

Inspection creates no additional global Store facts. The application deliberately
exports selected direct-method rows into a separate Store, joins them with a
module association and documentation presence read from actual Doc entries, and
queries undocumented selectors. It also retains answer-value history explicitly.
Captures run between
controlled edits; this does not claim an atomic arbitrary-world snapshot or a
complete documentation tool.

The Python checker expects the independent three-stage scenario: answers
20, 42, 10; direct selectors `answer`, then `answer/extra`, then `extra`; query
results empty, then `extra`, then `extra`. It rejects missing output, broken
inheritance, lost aliases/history and fact publication by inspection. The default
runner uses three fresh processes and writes a receipt and logs into a temporary
directory. Results include inspection CPU per phase, not startup or export/query
cost. They are workload observations, not a before/after speedup claim.

`tests/fixtures/reflect-owner.birnam` separately exercises snapshots during
collecting/reentrant visitors, owner replacement, Error propagation, identity
versus value equality and repeated inspection. Both are included in
`scripts/birnam-test.sh` (one application process in that gate).

Store still owns Globals storage and participates in method publication in this
first slice. The application establishes the replacement inspection interface;
it does not demonstrate a Store-free core yet. See
[the design and implementation record](../../docs/archive/2026-10-runtime-refactors/reflect-without-store.md).

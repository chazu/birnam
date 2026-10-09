# Result lifetime handoff investigation

The starting addition trace has 27 sends in the lifetime/cleanup category, with
additional protection attributed to the operations that produced their values.
Input borrowing, capture borrowing, local alias reuse and callback-free direct
return already remove many redundant registrations. The remaining values mostly
cross helper, getter or cleanup callbacks. They cannot all be borrowed from the
caller's inputs: newly allocated Words are not input aliases.

## Reproduced gap

`result_cleanup.rs` compiles a synchronous method that creates two Word results
and returns the second. A current `release:` wrapper forwards the actual release
then runs precise collection. The old finish path releases the result's token
first and then performs further cleanup. The result is reclaimed before the
caller receives it. This is a real production-path failing regression, not only
an abstract hypothetical. Conservative stack scanning can hide it; precise
collection exposes it. A C local alone is not a declared precise-GC root.

The invocation model already required no return gap, but that was a premise
about implementation correspondence. It did not prove the generated cleanup
kept this fresh result through every actual callback.

## Implemented owner handoff

Roots now exposes `releaseMany:protecting:batch:` with native token bytes, the
result Object and a native batch flag. It copies the token request before any
callback and establishes a nested private scope reported by `rootsDo:`. That
scope protects the result even after its original token is released.

The canonical binding guard admits callback-free owner batch removal for groups
of at least two tokens. Otherwise Roots sends every current `release:` in the
original reverse order. It continues all cleanup attempts after a release failure
and reports the first failure. The private scope drops on success/failure;
nested scopes restore the parent's suffix. No additional logical retention token
is created. Batch flag zero keeps the diagnostic control's individual sends.

Generated successful-result cleanup delegates directly to this owner operation,
including groups formerly cleaned by the unprotected bulk message. It resolves
selectors before borrowing the final token array, preserving cold-cache safety.
Root-free final returns remain ordinary direct Id dispatch with no cleanup call.
Error cleanup preserves the original body failure, continues after cleanup
failures and drains all tokens. A missing new cleanup operation causes a failure
and old-token drainage; it does not return an unprotected successful result.
Replacements of the new operation must honor its ownership-transfer contract.

Roots also shares the existing canonical `BindingGuard`: its former private
marshaller did not witness the matcher itself. The current matcher, lookup and
binding entries are now included with exact contexts. A forwarding matcher cannot
receive a positive canonical result and then mutate the release binding while
claiming eligibility. No saved entry is invoked; all module interactions remain
Id messages, and no IdApi/portable image structure changes.

## Measurement and remaining work

Addition drops from 177 to 175 sends: eight individual cleanup sends become three
protected cleanup messages and three binding proofs. Existing large cleanup
groups still cost two messages each. Three Word boxes and one Integer remain.
This is a small measured send improvement combined with a correctness repair;
no CPU speedup is claimed.

No remaining fresh-result retain is removed on the assumption that C locals or
caller inputs cover it. A wider call-scoped value inventory could replace more
per-result registrations, but must be traced through all callbacks, honor current
providers, preserve aliases and active code, and drain on every exit. That needs
an explicit invocation-owner design rather than another missing root.

Lean states that owner coverage survives cleanup collection. `ResultHandoff.tla`
models result-token removal, collecting callbacks, reverse cleanup order and
atomic caller transfer; missing/early scope and reordered variants must fail.
Native tests now cover the previously failing cleanup, token drainage after
failure, scope collection after exit, input aliases and the signed Integer oracle.
A C harness checks cold/full caches, direct returns and body-error precedence.
These are focused contracts and acceptance tests, not a proof of every native
provider or every compiler return path. Existing machine-code images retain their
embedded helper; loading them does not recompile them. Newly emitted code and
freshly built authored artifacts receive this fix.

Acceptance passed 9 focused Rust/C/native checks, all 143 current
Lean theorems/mutations and thirteen focused TLA+ configurations. The warmed
census measured 175 sends with stable inputs. See the [receipt](../measurements/result-lifetime-handoff-investigation-20261009.json).

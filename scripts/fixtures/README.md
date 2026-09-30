# Retained semantic fixtures

Current native integration runs through `rust/birnam-siphon/tests/` and
`scripts/native-message-test.sh`. Independent C consumers of the Id message
carrier and Execution descriptors live in `abi/fixtures/id-protocol-v1/`.

The former table-based construction, adoption, service-selection and image
fixtures are retired. They required deleted IdApi headers, Runtime constructors,
provider exports or diagnostic flags. Their relevant owner behavior is covered
through the production message boundary:

| Behavior | Current integration tests |
| --- | --- |
| Early module publication, values and layouts | `modules`, `object_model`, `numbers`, `numbers_loading`, `text_error` |
| Roots, activation, accounting, tracing and finalization | `roots`, `memory`, `memory_activation`, `allocation_accounting` |
| Store observation and graph traversal | `store`, `method_observation`, `storage_trace` |
| Loader and callback control | `native_loader`, `source_loader`, `block_loop`, `execution_methods`, `execution_machines` |
| Image graph, exact numeric bits, Error fields and source-free restoration | `image` and installed `host-image-test.py` |
| Nested source globals visible to previously compiled callers | installed `artifact-cli-test.py` |
| Retained Method invocation, receiver checks and native Unit reclamation | `execution_methods`, `materialization`, `image` |
| Bounded history, symbol churn and twenty fresh-process Image epochs | `store`, `memory`, `image` |

Deferred identity/index tables, prepaid allocation budgets, cached service
selection and optimizer-specific counters are retired contracts, not missing
message APIs. Deleting their probes does not establish optimizer equivalence or
performance. Their prior implementations remain in Git history.

The remaining Birnam execution, recovery, deoptimization, image and concurrency
fixtures retain useful semantic cases. Port those cases or establish their current
coverage before removing their old drivers; do not restore the deleted tables.

The former Runtime churn/longevity drivers required removed aggregate counters.
Their persistence and lifetime cases now run at these owners, with retained-history
and retained-symbol controls that demonstrate growth before reclamation. The old
RSS thresholds and optimizer performance gates are not claims of this migration.

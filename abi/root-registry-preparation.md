# Early root registration dependencies

Design [81BJ](../docs/design/81bj-production-root-provider-registry.md) moves
registration out of IdApi without moving the early registration points.

- `birnam_string_prepare_v3(api, exports, allocation, roots)` appends the incoming
  root registry to String preparation v2. The instance is published before
  registration and populated afterwards. The String v1 operation table is unchanged.
- `birnam_float_prepare_v5(api, presence, imports, exports, roots)` and
  `birnam_word_prepare_v5(api, presence, imports, exports, roots)` append the
  incoming capability to preparation v4. Their instance publication, registration,
  population, repeat checks and recursive preparation rejection stay in order.
  Float v1 and Word v1 operation tables are unchanged.

The [String](fixtures/string-prepare-v3/string_prepare_v3.h),
[Float](fixtures/float-prepare-v5/float_prepare_v5.h) and
[Word](fixtures/word-prepare-v5/word_prepare_v5.h) C mirrors fix those signatures.
Each supplied capability, context and code remains live with its matching API.
These are explicit early host dependencies, before ordinary module mounting;
there is no fallback through an IdApi registration slot. Existing representations,
layouts, error behavior and image codecs remain unchanged.

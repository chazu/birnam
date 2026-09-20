# Isolate native OOP bridges v2

Design [81BE](../docs/design/81be-production-namespace-import.md) passes the
caller's live Namespace import capability beside the caller's IdApi:

```c
int birnam_isolate_worker_driver_v2(IdApi *api, const NamespaceImportV1 *imports,
                                  int fd, Oop receiver, Oop selector);
int birnam_isolate_rust_encode_oop_v2(IdApi *api, const NamespaceImportV1 *imports,
                                    Oop value, uint8_t *out, size_t capacity,
                                    size_t *out_length);
int birnam_isolate_rust_decode_oop_v2(IdApi *api, const NamespaceImportV1 *imports,
                                    const uint8_t *bytes, size_t length, Oop *out);
```

The borrowed API, capability, context, code and referenced objects stay valid
for each call. Existing buffer, owner-thread and rooting obligations remain.
The encoder, decoder and worker use these incoming imports at their original
Vector, Map and Globals lookup points. Each import selects the current
function/context together; decoded identities and callback result snapshots
retain their prior meaning. Isolate's ordinary methods retain the capability
captured with their first execution API, independently of these native callers.

The worker descriptor remains caller-owned and is not closed by this entry.
Wire-v1 bytes, limits, error codes, result handling and worker lifecycle are
unchanged. The three unversioned bridge exports are replaced, without adapters;
callers must rebuild. Header codec exports and `birnam_isolate_worker_boot_error`
retain their signatures because they do not use imports.

Run `python3 scripts/isolate-image-wire-test.py PREFIX --baseline OLD_PREFIX`
for an independent actual-worker wire comparison after source removal. The
external test reader can wait for debug boot; the ordinary Isolate parent's
production ready-handshake deadline remains ten seconds.

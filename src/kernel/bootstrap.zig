//! Kernel bootstrap: brings up the id object model atop the fact store.
//!
//! Stages (VISION.md §Bootstrap, Phase 4 design):
//!   1. Allocate kernel vtable structs (zero user slots each).
//!   2. Wire vt_ref at offset -1 of every kernel vtable to mkEid(VTABLE_VT).
//!      vtable_vt's own -1 slot resolves to itself (fixed-point).
//!   3. Register every kernel-vtable pointer Ref in the object table.
//!   4. Assert each kernel vtable's `:vtable` fact (so bind's cold path
//!      can dispatch on EID-tagged refs to vtables).
//!   5. Assert `:delegate` facts for the kernel delegation chain.
//!      Absence of a `:delegate` fact = top of chain (Date discipline).
//!   6. Assert `[NIL :vtable NIL_VT]` so `send`-to-NIL is legal.
//!   7. For each kernel method (vt, sel, fn): allocate a Closure struct,
//!      mint EID, register, assert the method fact.
//!   8. initDispatch sets the send.zig globals so bind can run.

const std = @import("std");
const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const closure_mod = @import("closure.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");
const send_mod = @import("send.zig");
const mcache = @import("mcache.zig");
const interner_mod = @import("interner.zig");
const rule_mod = @import("rule.zig");
const predicate_mod = @import("predicate.zig");
const promotion_mod = @import("promotion.zig");
const ffi_module_mod = @import("ffi_module.zig");
const loader_mod = @import("loader.zig");
const macho_mod = @import("macho.zig");
const elf_mod = @import("elf.zig");
const id_api_mod = @import("id_api.zig");

const Ref = ref_mod.Ref;
const Closure = closure_mod.Closure;
const Send = send_mod.Send;

/// Kernel handles, returned for caller convenience and testing.
pub const Kernel = struct {
    vtable_vt_ref: Ref,
    object_vt_ref: Ref,
    selector_vt_ref: Ref,
    closure_vt_ref: Ref,
    vector_vt_ref: Ref,
    assoc_vt_ref: Ref,
    nil_vt_ref: Ref,
    float_vt_ref: Ref,
    ffi_ptr_vt_ref: Ref,
    module_vt_ref: Ref,
    error_vt_ref: Ref,
};

/// Bootstrap the kernel. After this call:
///   - send / bind are wired and operational
///   - kernel vtables are materialized + registered
///   - kernel methods are installed and dispatchable
///   - kernel selectors carry `:name` facts so `intern("lookup:") -> SEL_LOOKUP`
/// Module-global rule registry. The shell creates one and the bootstrap
/// hands it to the IdApi layer. Modules add rules via `api.define_rule`.
pub fn boot(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
) !Kernel {
    // Phase 7.1: provision a kernel-side rule registry for IdApi.
    const registry = try allocator.create(rule_mod.Registry);
    registry.* = rule_mod.Registry.init(allocator, store, table);
    id_api_mod.setRegistry(registry);
    // Wire mcache invalidation hook into the fact store.
    store.on_attr_mutate = &send_mod.onAttrMutate;

    // Wire fact-store EID allocator for lazy reification (Phase 7.3).
    store.setEidAllocTable(table);

    // -- Stage 1+2+3: allocate kernel vtables, set -1, register ---------
    const vt_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.VTABLE_VT, object.toRef(vt_vt_ptr));

    const obj_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.OBJECT_VT, object.toRef(obj_vt_ptr));

    const sel_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.SELECTOR_VT, object.toRef(sel_vt_ptr));

    const clo_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.CLOSURE_VT, object.toRef(clo_vt_ptr));

    const vec_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.VECTOR_VT, object.toRef(vec_vt_ptr));

    const ass_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.ASSOC_VT, object.toRef(ass_vt_ptr));

    const nil_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.NIL_VT, object.toRef(nil_vt_ptr));

    const rrb_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.RRB_VT, object.toRef(rrb_vt_ptr));

    const float_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.FLOAT_VT, object.toRef(float_vt_ptr));

    const ffi_ptr_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.FFI_PTR_VT, object.toRef(ffi_ptr_vt_ptr));

    const module_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.MODULE_VT, object.toRef(module_vt_ptr));

    const error_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.ERROR_VT, object.toRef(error_vt_ptr));

    // -- Stage 4: assert :vtable facts for every entity bind may dispatch on
    // Each kernel vtable's :vtable is VTABLE_VT (so sending a message to
    // mkEid(SOME_VT) lands in vtable_vt's method table).
    const m_boot = kernel_eids.M_BOOTSTRAP;
    const vtv = ref_mod.mkEid(kernel_eids.VTABLE_VT);
    _ = try store.assert(kernel_eids.VTABLE_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.OBJECT_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.SELECTOR_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.CLOSURE_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.VECTOR_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.ASSOC_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.NIL_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.RRB_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.FLOAT_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.FFI_PTR_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.MODULE_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.ERROR_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);

    // -- Stage 5: delegation chain ---------------------------------------
    // Top of chain (object_vt) has NO :delegate fact — Date: absence = no parent.
    // Everyone else delegates to object_vt.
    const ovt = ref_mod.mkEid(kernel_eids.OBJECT_VT);
    _ = try store.assert(kernel_eids.VTABLE_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.SELECTOR_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.CLOSURE_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.VECTOR_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.ASSOC_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.NIL_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.RRB_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.FLOAT_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.FFI_PTR_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.MODULE_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.ERROR_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.RANGE_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot); // Phase 16: seq protocol
    _ = try store.assert(kernel_eids.STRING_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot); // Phase 16: strings
    _ = try store.assert(kernel_eids.CELL_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot); // Phase 16 Tier E: cells
    _ = try store.assert(kernel_eids.XF_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot); // Phase 16 Tier E: transducer factory

    // -- Stage 6: NIL's :vtable ------------------------------------------
    _ = try store.assert(kernel_eids.NIL, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.NIL_VT), m_boot);

    // -- Stage 7: install kernel methods ---------------------------------
    try installMethod(allocator, store, table, kernel_eids.OBJECT_VT, kernel_eids.SEL_VTABLE, @ptrCast(&objectVtableImpl));
    try installMethod(allocator, store, table, kernel_eids.OBJECT_VT, kernel_eids.SEL_INIT, @ptrCast(&objectInitImpl));
    try installMethod(allocator, store, table, kernel_eids.VTABLE_VT, kernel_eids.SEL_LOOKUP, @ptrCast(&vtableLookupImpl));
    try installMethod(allocator, store, table, kernel_eids.VTABLE_VT, kernel_eids.SEL_DELEGATED, @ptrCast(&vtableDelegatedImpl));
    try installMethod(allocator, store, table, kernel_eids.VTABLE_VT, kernel_eids.SEL_METHOD_AT_PUT_WITH, @ptrCast(&vtableMethodAtPutWithImpl));

    // -- Stage 8: seed selector names ------------------------------------
    try interner.seed(store, kernel_eids.SEL_LOOKUP, "lookup:");
    try interner.seed(store, kernel_eids.SEL_METHOD_AT_PUT_WITH, "methodAt:put:with:");
    try interner.seed(store, kernel_eids.SEL_DELEGATED, "delegated");
    try interner.seed(store, kernel_eids.SEL_INIT, "init");
    try interner.seed(store, kernel_eids.SEL_DNU, "doesNotUnderstand:");
    try interner.seed(store, kernel_eids.SEL_ALLOCATE, "allocate");
    try interner.seed(store, kernel_eids.SEL_VTABLE, "vtable");
    try interner.seed(store, kernel_eids.ATTR_SAVED_QUERY_NAME, "saved-query-name");
    try interner.seed(store, kernel_eids.ATTR_SAVED_QUERY_SPEC, "saved-query-spec");
    try interner.seed(store, kernel_eids.ATTR_RULE_NAME, "rule-name");
    try interner.seed(store, kernel_eids.ATTR_RULE_PARAMS, "rule-params");
    try interner.seed(store, kernel_eids.ATTR_RULE_MAX_VAR, "rule-max-var");
    try interner.seed(store, kernel_eids.ATTR_RULE_BODY, "rule-body");
    try interner.seed(store, kernel_eids.ATTR_PRED_NAME, "predicate-name");
    try interner.seed(store, kernel_eids.ATTR_PRED_IMPL, "predicate-impl");
    try interner.seed(store, kernel_eids.ATTR_FN_NAME, "function-name");
    try interner.seed(store, kernel_eids.ATTR_FN_IMPL, "function-impl");
    try interner.seed(store, kernel_eids.ATTR_PROMOTE_THRESHOLD, "promote-threshold");
    try interner.seed(store, kernel_eids.ATTR_BIND, "bind-impl");
    try interner.seed(store, kernel_eids.SEL_BIND, "bind:");
    try interner.seed(store, kernel_eids.ATTR_FFI_ADDR, "ffi-addr");
    try interner.seed(store, kernel_eids.ATTR_FFI_SIG, "ffi-sig");
    try interner.seed(store, kernel_eids.ATTR_CARDINALITY, "cardinality");
    try interner.seed(store, kernel_eids.ATTR_FFI_HAS_SYMBOL, "ffi-has-symbol");
    try interner.seed(store, kernel_eids.ATTR_FFI_SYMBOL_NAME, "ffi-symbol-name");
    try interner.seed(store, kernel_eids.ATTR_FFI_SYMBOL_ADDR, "ffi-symbol-addr");
    try interner.seed(store, kernel_eids.ATTR_FFI_SYMBOL_KIND, "ffi-symbol-kind");
    try interner.seed(store, kernel_eids.ATTR_FFI_SYMBOL_EXTERNAL, "ffi-symbol-external?");
    try interner.seed(store, kernel_eids.ATTR_FFI_SYMBOL_MODULE, "ffi-symbol-module");
    try interner.seed(store, kernel_eids.ATTR_FFI_SYMBOLS_ENUMERATED, "ffi-symbols-enumerated?");
    try interner.seed(store, kernel_eids.SEL_SYMBOLS, "symbols");
    try interner.seed(store, kernel_eids.ATTR_READER_MAGIC, "reader-magic");
    try interner.seed(store, kernel_eids.ATTR_READER_IMPL, "reader-impl");
    try interner.seed(store, kernel_eids.ATTR_FFI_OWNER, "ffi-owner");
    try interner.seed(store, kernel_eids.ATTR_FFI_STALE, "ffi-stale?");
    try interner.seed(store, kernel_eids.SEL_UNLOAD, "unload");
    try interner.seed(store, kernel_eids.ATTR_FFI_HANDLE, "ffi-handle");
    try interner.seed(store, kernel_eids.ATTR_FFI_PATH, "ffi-path");
    try interner.seed(store, kernel_eids.ATTR_ERROR_KIND, "error-kind");
    try interner.seed(store, kernel_eids.ATTR_ERROR_MESSAGE, "error-message");
    try interner.seed(store, kernel_eids.ATTR_ERROR_PATH, "error-path");
    try interner.seed(store, kernel_eids.ATTR_ERROR_NAME, "error-name");
    try interner.seed(store, kernel_eids.ATTR_ERROR_ARITY, "error-arity");
    try interner.seed(store, kernel_eids.ATTR_ERROR_DETAIL, "error-detail");
    try interner.seed(store, kernel_eids.ATTR_FFI_WRAP_SYMBOL, "ffi-wrap-symbol");
    try interner.seed(store, kernel_eids.ATTR_FFI_SIG_STR, "ffi-sig-str");
    try interner.seed(store, kernel_eids.ATTR_WRAP_SELECTOR, "wrap-selector");
    try interner.seed(store, kernel_eids.SEL_RESOLVE, "resolve");
    try interner.seed(store, kernel_eids.ATTR_FILED_IN_PATH, "filed-in-path");
    try interner.seed(store, kernel_eids.ATTR_FILED_IN_SOURCE, "filed-in-source");
    try interner.seed(store, kernel_eids.SEL_LOAD, "load:");
    try interner.seed(store, kernel_eids.SEL_WRAP_AS_SIG, "wrap:as:signature:");
    try interner.seed(store, kernel_eids.SEL_PATH, "path");
    try interner.seed(store, kernel_eids.SEL_HANDLE, "handle");

    // Phase 18 prelude: FFI memory primitives.
    try interner.seed(store, kernel_eids.SEL_FFI_ALLOC, "alloc:");
    try interner.seed(store, kernel_eids.SEL_FFI_FREE, "free");
    try interner.seed(store, kernel_eids.SEL_FFI_ADDRESS, "address");
    try interner.seed(store, kernel_eids.SEL_FFI_IS_NULL, "isNull");
    try interner.seed(store, kernel_eids.SEL_FFI_PEEK_INT, "peekInt:");
    try interner.seed(store, kernel_eids.SEL_FFI_PEEK_BYTE, "peekByte:");
    try interner.seed(store, kernel_eids.SEL_FFI_PEEK_DOUBLE, "peekDouble:");
    try interner.seed(store, kernel_eids.SEL_FFI_PEEK_PTR, "peekPtr:");
    try interner.seed(store, kernel_eids.SEL_FFI_PEEK_STRING, "peekString:");
    try interner.seed(store, kernel_eids.SEL_FFI_POKE_INT, "pokeInt:at:");
    try interner.seed(store, kernel_eids.SEL_FFI_POKE_BYTE, "pokeByte:at:");
    try interner.seed(store, kernel_eids.SEL_FFI_POKE_DOUBLE, "pokeDouble:at:");
    try interner.seed(store, kernel_eids.SEL_FFI_POKE_PTR, "pokePtr:at:");
    // Phase 19.3 — char** string arrays.
    try interner.seed(store, kernel_eids.SEL_FFI_ARGV, "argv:");
    try interner.seed(store, kernel_eids.SEL_FFI_PEEK_STRING_ARRAY, "peekStringArray:");
    // Phase 19.4 — bulk byte buffers.
    try interner.seed(store, kernel_eids.SEL_FFI_PEEK_BYTES, "peekBytes:len:");
    try interner.seed(store, kernel_eids.SEL_FFI_POKE_BYTES, "pokeBytes:at:");
    // Phase 19.7 — out-param cells. `value` reuses SEL_VALUE (seeded below).
    try interner.seed(store, kernel_eids.SEL_FFI_OUT_INT, "outInt");
    try interner.seed(store, kernel_eids.SEL_FFI_OUT_LONG, "outLong");
    try interner.seed(store, kernel_eids.SEL_FFI_OUT_PTR, "outPtr");
    try interner.seed(store, kernel_eids.SEL_FFI_OUT_DOUBLE, "outDouble");

    // -- Stage 9: enable dispatch ----------------------------------------
    send_mod.initDispatch(store, table, allocator);

    // -- Stage 10: register Phase 7.4 built-in functions ------------------
    try predicate_mod.registerBuiltins(allocator, store, table, interner);

    // -- Stage 11: Phase 8 promotion init --------------------------------
    // Seed the default threshold BEFORE installing the promotion hook so
    // the hook doesn't fire on its own seed write.
    _ = try store.assert(
        kernel_eids.M_BOOTSTRAP,
        kernel_eids.ATTR_PROMOTE_THRESHOLD,
        ref_mod.mkSmallInt(100).?,
        kernel_eids.M_BOOTSTRAP,
    );
    try promotion_mod.init(allocator, store, table);

    // -- Stage 12: install Phase 9.5 Module methods on MODULE_VT ---------
    try ffi_module_mod.installMethods(allocator, store, table);

    // -- Stage 13: Phase 9.5.2 cardinality schema injection --------------
    // Intern "many" so its EID is stable, then hand the fact store the
    // (attr_eid, many_value_ref) pair so assert/currentValue can branch.
    const many_eid = try interner.intern(store, table, "many");
    _ = try interner.intern(store, table, "one");
    store.setCardinalitySchema(.{
        .attr_eid = kernel_eids.ATTR_CARDINALITY,
        .many_value = ref_mod.mkEid(many_eid),
    });

    // -- Stage 14: declare :many attrs (Phase 9.5.3+) --------------------
    // `:ffi-has-symbol` accumulates one fact per enumerated symbol per Module.
    _ = try store.assert(
        kernel_eids.ATTR_FFI_HAS_SYMBOL,
        kernel_eids.ATTR_CARDINALITY,
        ref_mod.mkEid(many_eid),
        kernel_eids.M_BOOTSTRAP,
    );

    // -- Stage 15: register binary-format readers (Phase 9.5.4) ----------
    // Fact-dispatched registry: format = magic-bytes; impl = fn ptr Ref.
    // Adding a future format = one `registerReader` call (or even just
    // asserting two facts from a user module).
    try loader_mod.registerReader(allocator, store, table, macho_mod.MAGIC_64, &macho_mod.openSymbolsAdapter);
    try loader_mod.registerReader(allocator, store, table, elf_mod.MAGIC, &elf_mod.openSymbolsAdapter);

    // -- Stage 16: Phase 10.0 kernel vtables + booleans + PEG slots -----
    // All the vts reserved for Phase 10 work are pre-materialized here so
    // their EIDs are usable by lang dylib / PEG dylib at load time. No
    // methods installed yet — those live in their respective dylibs (lang
    // for booleans/blocks/smallint arithmetic, PEG for grammar dispatch).
    const block_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.BLOCK_VT, object.toRef(block_vt_ptr));
    const smallint_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.SMALLINT_VT, object.toRef(smallint_vt_ptr));
    const boolean_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.BOOLEAN_VT, object.toRef(boolean_vt_ptr));
    const true_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.TRUE_VT, object.toRef(true_vt_ptr));
    const false_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.FALSE_VT, object.toRef(false_vt_ptr));
    const grammar_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.GRAMMAR_VT, object.toRef(grammar_vt_ptr));
    const peg_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.PEG_VT, object.toRef(peg_vt_ptr));

    // :vtable facts for each new vt (so [SMALLINT_VT vtable] etc dispatch correctly)
    _ = try store.assert(kernel_eids.BLOCK_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.SMALLINT_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.BOOLEAN_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.TRUE_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.FALSE_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.GRAMMAR_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.PEG_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);

    // :delegate chain: BOOLEAN_VT, BLOCK_VT, SMALLINT_VT, GRAMMAR_VT, PEG_VT
    // all delegate directly to OBJECT_VT. TRUE_VT and FALSE_VT delegate to
    // BOOLEAN_VT (common-delegate pattern from Phase 10 decision MD4 area).
    const boolean_vt_ref = ref_mod.mkEid(kernel_eids.BOOLEAN_VT);
    _ = try store.assert(kernel_eids.BLOCK_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.SMALLINT_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.BOOLEAN_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.TRUE_VT, kernel_eids.ATTR_DELEGATE, boolean_vt_ref, m_boot);
    _ = try store.assert(kernel_eids.FALSE_VT, kernel_eids.ATTR_DELEGATE, boolean_vt_ref, m_boot);
    _ = try store.assert(kernel_eids.GRAMMAR_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.PEG_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);

    // TRUE_EID and FALSE_EID singletons: distinct EIDs from their vts
    // (matches NIL / NIL_VT precedent). Asserted with :vtable so EID-tag
    // dispatch on the singleton lands in TRUE_VT / FALSE_VT.
    _ = try store.assert(kernel_eids.TRUE_EID, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.TRUE_VT), m_boot);
    _ = try store.assert(kernel_eids.FALSE_EID, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.FALSE_VT), m_boot);

    // PEG_EID singleton — pattern-constructor receiver. `[PEG literal: ...]`
    // dispatches against PEG_VT's installed methods (10.0 step 5).
    _ = try store.assert(kernel_eids.PEG_EID, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.PEG_VT), m_boot);

    // -- Stage 17: Phase 10.1 lang-module kernel slots ------------------
    // All vts pre-materialized as empty prototypes; methods install at
    // lang-dylib init (or via the user via [vt method:do:] in user code).
    // Delegation: every new vt → OBJECT_VT (top of chain).
    const phase_10_1_vts = [_]u64{
        kernel_eids.BINDING_VT,
        kernel_eids.SCOPE_VT,
        kernel_eids.LANG_VT,
        kernel_eids.AST_LITERAL_VT,
        kernel_eids.AST_VAR_REF_VT,
        kernel_eids.AST_SELF_VT,
        kernel_eids.AST_KEYWORD_SEND_VT,
        kernel_eids.AST_UNARY_SEND_VT,
        kernel_eids.AST_BINARY_SEND_VT,
        kernel_eids.AST_PATTERN_SEND_VT,
        kernel_eids.AST_BLOCK_VT,
        kernel_eids.AST_LET_VT,
        kernel_eids.AST_ASSIGN_VT,
        kernel_eids.AST_RETURN_VT,
        kernel_eids.AST_SUPER_SEND_VT,
        kernel_eids.AST_STMT_SEQ_VT,
    };
    for (phase_10_1_vts) |vt_eid| {
        const ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
        try table.put(vt_eid, object.toRef(ptr));
        _ = try store.assert(vt_eid, kernel_eids.ATTR_VTABLE, vtv, m_boot);
        _ = try store.assert(vt_eid, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    }

    // LANG_EID singleton — language-module factory receiver. Methods on
    // LANG_VT dispatch when user code sends to the bound `Lang` name (10.4+).
    _ = try store.assert(kernel_eids.LANG_EID, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.LANG_VT), m_boot);

    // Seed AST attr + binding/scope attr names so user code can intern them
    // by string and recover the kernel EID.
    try interner.seed(store, kernel_eids.ATTR_AST_VALUE, "ast-value");
    try interner.seed(store, kernel_eids.ATTR_AST_NAME, "ast-name");
    try interner.seed(store, kernel_eids.ATTR_AST_RECEIVER, "ast-receiver");
    try interner.seed(store, kernel_eids.ATTR_AST_SELECTOR, "ast-selector");
    try interner.seed(store, kernel_eids.ATTR_AST_FIRST_ARG, "ast-first-arg");
    try interner.seed(store, kernel_eids.ATTR_AST_BLOCK_PARAMS, "ast-block-params");
    try interner.seed(store, kernel_eids.ATTR_AST_BLOCK_BODY, "ast-block-body");
    try interner.seed(store, kernel_eids.ATTR_AST_LET_INIT, "ast-let-init");
    try interner.seed(store, kernel_eids.ATTR_AST_ASSIGN_TARGET, "ast-assign-target");
    try interner.seed(store, kernel_eids.ATTR_AST_ASSIGN_VALUE, "ast-assign-value");
    try interner.seed(store, kernel_eids.ATTR_AST_RETURN_VALUE, "ast-return-value");
    try interner.seed(store, kernel_eids.ATTR_AST_BIND_DEPTH, "ast-bind-depth");
    try interner.seed(store, kernel_eids.ATTR_AST_BIND_SLOT, "ast-bind-slot");
    try interner.seed(store, kernel_eids.ATTR_AST_BIND_NAME, "ast-bind-name");
    try interner.seed(store, kernel_eids.ATTR_AST_METHOD_FRAME, "ast-method-frame");
    try interner.seed(store, kernel_eids.ATTR_AST_SUPER_VT, "ast-super-vt");
    try interner.seed(store, kernel_eids.ATTR_AST_STMT_FIRST, "ast-stmt-first");

    try interner.seed(store, kernel_eids.ATTR_BIND_SCOPE, "bind-scope");
    try interner.seed(store, kernel_eids.ATTR_BIND_NAME, "bind-name");
    try interner.seed(store, kernel_eids.ATTR_BIND_VALUE, "bind-value");
    try interner.seed(store, kernel_eids.ATTR_SCOPE_PARENT, "scope-parent");
    try interner.seed(store, kernel_eids.ATTR_SCOPE_TOP_LEVEL, "scope-top-level");

    // Seed control-flow + arithmetic + boolean selectors so lang dylib (and
    // user code) can `intern("+")` etc and get the kernel-fixed EID.
    try interner.seed(store, kernel_eids.SEL_VALUE, "value");
    try interner.seed(store, kernel_eids.SEL_VALUE_OF, "value:");
    try interner.seed(store, kernel_eids.SEL_IF_TRUE, "ifTrue:");
    try interner.seed(store, kernel_eids.SEL_IF_FALSE, "ifFalse:");
    try interner.seed(store, kernel_eids.SEL_IF_TRUE_IF_FALSE, "ifTrue:ifFalse:");
    try interner.seed(store, kernel_eids.SEL_WHILE_TRUE, "whileTrue:");
    try interner.seed(store, kernel_eids.SEL_WHILE_FALSE, "whileFalse:");
    try interner.seed(store, kernel_eids.SEL_TIMES_REPEAT, "timesRepeat:");
    try interner.seed(store, kernel_eids.SEL_TO_DO, "to:do:");
    try interner.seed(store, kernel_eids.SEL_NOT, "not");
    try interner.seed(store, kernel_eids.SEL_AND, "and:");
    try interner.seed(store, kernel_eids.SEL_OR, "or:");
    try interner.seed(store, kernel_eids.SEL_PLUS, "+");
    try interner.seed(store, kernel_eids.SEL_MINUS, "-");
    try interner.seed(store, kernel_eids.SEL_TIMES, "*");
    try interner.seed(store, kernel_eids.SEL_DIVIDE, "/");
    try interner.seed(store, kernel_eids.SEL_MOD, "mod");
    try interner.seed(store, kernel_eids.SEL_EQ, "=");
    try interner.seed(store, kernel_eids.SEL_LT, "<");
    try interner.seed(store, kernel_eids.SEL_GT, ">");
    try interner.seed(store, kernel_eids.SEL_LE, "<=");
    try interner.seed(store, kernel_eids.SEL_GE, ">=");

    // Phase 10.4.5 scope attrs.
    try interner.seed(store, kernel_eids.ATTR_SCOPE_SELF, "scope-self");
    try interner.seed(store, kernel_eids.ATTR_SCOPE_FIRST_BINDING, "scope-first-binding");
    try interner.seed(store, kernel_eids.ATTR_BIND_NEXT, "bind-next");

    // Seed Phase 10 selector + attribute names so user code can intern them
    // by string and recover the kernel EID (Phase 4.5 interner pattern).
    try interner.seed(store, kernel_eids.SEL_DEFINE_RULE_AS, "defineRule:as:");
    try interner.seed(store, kernel_eids.SEL_PARSE_FROM, "parse:from:");
    try interner.seed(store, kernel_eids.ATTR_PATTERN_KIND, "pattern-kind");
    try interner.seed(store, kernel_eids.ATTR_PATTERN_FIRST_CHILD, "pattern-first-child");
    try interner.seed(store, kernel_eids.ATTR_PATTERN_NEXT_SIBLING, "pattern-next-sibling");
    try interner.seed(store, kernel_eids.ATTR_LITERAL_VALUE, "literal-value");
    try interner.seed(store, kernel_eids.ATTR_RULE_REF_NAME, "rule-ref-name");
    try interner.seed(store, kernel_eids.ATTR_CHARSET_FIRST_RANGE, "charset-first-range");
    try interner.seed(store, kernel_eids.ATTR_CHARSET_NEXT_RANGE, "charset-next-range");
    try interner.seed(store, kernel_eids.ATTR_CHARSET_RANGE_LO, "charset-range-lo");
    try interner.seed(store, kernel_eids.ATTR_CHARSET_RANGE_HI, "charset-range-hi");
    try interner.seed(store, kernel_eids.ATTR_GRAMMAR_FIRST_RULE, "grammar-first-rule");
    try interner.seed(store, kernel_eids.ATTR_RULE_NEXT, "rule-next");
    try interner.seed(store, kernel_eids.ATTR_RULE_PATTERN, "rule-pattern");
    try interner.seed(store, kernel_eids.ATTR_AST_RULE_NAME, "ast-rule-name");
    try interner.seed(store, kernel_eids.ATTR_AST_FIRST_CHILD, "ast-first-child");
    try interner.seed(store, kernel_eids.ATTR_AST_NEXT_SIBLING, "ast-next-sibling");
    try interner.seed(store, kernel_eids.ATTR_AST_BYTE_START, "ast-byte-start");
    try interner.seed(store, kernel_eids.ATTR_AST_BYTE_END, "ast-byte-end");
    try interner.seed(store, kernel_eids.ATTR_AST_PARSE, "ast-parse");
    try interner.seed(store, kernel_eids.ATTR_PEG_EXPECTED, "peg-expected");
    try interner.seed(store, kernel_eids.ATTR_PEG_POSITION, "peg-position");

    // Phase 10.0: :peg-expected is :many cardinality (multiple expected names
    // at the furthest-failure position).
    _ = try store.assert(
        kernel_eids.ATTR_PEG_EXPECTED,
        kernel_eids.ATTR_CARDINALITY,
        ref_mod.mkEid(many_eid),
        kernel_eids.M_BOOTSTRAP,
    );

    // Also intern the bare boolean names so user-source `true` / `false` AST
    // literal-nodes can resolve to the singleton EIDs at parse time.
    try interner.seed(store, kernel_eids.TRUE_EID, "true");
    try interner.seed(store, kernel_eids.FALSE_EID, "false");

    return .{
        .vtable_vt_ref = object.toRef(vt_vt_ptr),
        .object_vt_ref = object.toRef(obj_vt_ptr),
        .selector_vt_ref = object.toRef(sel_vt_ptr),
        .closure_vt_ref = object.toRef(clo_vt_ptr),
        .vector_vt_ref = object.toRef(vec_vt_ptr),
        .assoc_vt_ref = object.toRef(ass_vt_ptr),
        .nil_vt_ref = object.toRef(nil_vt_ptr),
        .float_vt_ref = object.toRef(float_vt_ptr),
        .ffi_ptr_vt_ref = object.toRef(ffi_ptr_vt_ptr),
        .module_vt_ref = object.toRef(module_vt_ptr),
        .error_vt_ref = object.toRef(error_vt_ptr),
    };
}

// -- Allocation helpers -------------------------------------------------------

fn allocVtStruct(allocator: std.mem.Allocator, vt_eid: u64) !object.ObjPtr {
    _ = vt_eid;
    // Every kernel vtable has 0 user slots — its content is fact-shaped.
    // -1 slot is set to mkEid(VTABLE_VT): all kernel vtables share the metavtable.
    return object.allocObject(allocator, ref_mod.mkEid(kernel_eids.VTABLE_VT), 0);
}

/// Allocate a Closure as a libid-style object: layout `[vt_ref][method][data]`,
/// where vt_ref = mkEid(CLOSURE_VT) at offset -1. Closure pointer is at offset 0.
fn allocClosure(allocator: std.mem.Allocator, method_fn: *const anyopaque, data_ref: Ref) !*Closure {
    const total = 3 * @sizeOf(Ref);
    const buf = try allocator.alignedAlloc(u8, .of(Ref), total);
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
    const c: *Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c.method = method_fn;
    c.data = data_ref;
    return c;
}

fn installMethod(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    vt_eid: u64,
    sel_eid: u64,
    method_fn: *const anyopaque,
) !void {
    const c = try allocClosure(allocator, method_fn, ref_mod.NIL);
    const closure_eid = table.alloc();
    try table.put(closure_eid, ref_mod.mkPointer(@ptrCast(c)));
    _ = try store.assert(vt_eid, sel_eid, ref_mod.mkEid(closure_eid), kernel_eids.M_BOOTSTRAP);
}

// -- Kernel method implementations -------------------------------------------

/// `[obj vtable]` — return the receiver's vtable Ref.
/// Works for pointer receivers (reads -1) and EID receivers (queries :vtable).
fn objectVtableImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    if (ref_mod.isPointer(s.receiver)) {
        return object.vtableOf(object.fromRef(s.receiver));
    }
    if (ref_mod.isEid(s.receiver)) {
        // The caller already routed through bind, which proved a vt exists.
        // Re-derive it.
        const store = send_mod.storePtr() orelse return ref_mod.NIL;
        const e = ref_mod.untagEid(s.receiver);
        return store.currentValue(e, kernel_eids.ATTR_VTABLE) orelse ref_mod.NIL;
    }
    return ref_mod.NIL;
}

/// `[obj init]` — identity.
fn objectInitImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    return s.receiver;
}

/// `[vt lookup: sel]` — return the method Ref currently bound to `sel` on `vt`,
/// or NIL if absent. Walks the delegation chain, mirroring `bind`'s cold path.
fn vtableLookupImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const sel_ref = args[0];
    if (!ref_mod.isEid(s.receiver) or !ref_mod.isEid(sel_ref)) return ref_mod.NIL;
    const sel_eid = ref_mod.untagEid(sel_ref);
    var cur_vt = ref_mod.untagEid(s.receiver);
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    while (cur_vt != kernel_eids.NIL) {
        if (store.currentValue(cur_vt, sel_eid)) |m| return m;
        if (store.currentValue(cur_vt, kernel_eids.ATTR_DELEGATE)) |d| {
            if (!ref_mod.isEid(d)) return ref_mod.NIL;
            cur_vt = ref_mod.untagEid(d);
        } else return ref_mod.NIL;
    }
    return ref_mod.NIL;
}

/// `[vt delegated]` — allocate a fresh child vtable delegating to `vt`.
/// Returns the new vtable's EID Ref.
fn vtableDelegatedImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    if (!ref_mod.isEid(s.receiver)) return ref_mod.NIL;
    const parent_eid = ref_mod.untagEid(s.receiver);
    const allocator = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const table = send_mod.tablePtr() orelse return ref_mod.NIL;

    const new_struct = allocVtStruct(allocator, 0) catch return ref_mod.NIL;
    const new_eid = table.alloc();
    table.put(new_eid, object.toRef(new_struct)) catch return ref_mod.NIL;

    // The child's :vtable is VTABLE_VT; its :delegate is the parent.
    _ = store.assert(new_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.VTABLE_VT), kernel_eids.M_BOOTSTRAP) catch return ref_mod.NIL;
    _ = store.assert(new_eid, kernel_eids.ATTR_DELEGATE, ref_mod.mkEid(parent_eid), kernel_eids.M_BOOTSTRAP) catch return ref_mod.NIL;

    return ref_mod.mkEid(new_eid);
}

/// `[vt methodAt: sel put: method_ref with: data]` — install a method on `vt`.
/// `method_ref` must already be an EID Ref to a registered closure. `data` is
/// ignored in this Phase 4 stub (closures carry their own data at allocation).
fn vtableMethodAtPutWithImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const sel_ref = args[0];
    const method_ref = args[1];
    // data_ref = args[2] — ignored
    if (!ref_mod.isEid(s.receiver) or !ref_mod.isEid(sel_ref) or !ref_mod.isEid(method_ref)) return ref_mod.NIL;
    const vt_eid = ref_mod.untagEid(s.receiver);
    const sel_eid = ref_mod.untagEid(sel_ref);
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    _ = store.assert(vt_eid, sel_eid, method_ref, kernel_eids.M_BOOTSTRAP) catch return ref_mod.NIL;
    return method_ref;
}

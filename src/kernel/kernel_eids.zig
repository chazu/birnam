//! Well-known kernel EIDs.
//!
//! Locked in VISION.md §Open Questions: kernel EIDs occupy 0..255;
//! user EIDs allocate from 256+. Renumbering this block is a
//! breaking image-format change — accepted in Phase 9.5.5 (no image
//! artifacts exist yet, Phase 12 has not shipped).
//!
//! Layout convention:
//!   0        NIL (the nothing-entity)
//!   1..15    kernel vtables
//!   16..63   kernel selectors and well-known attributes (Phase 0-9.5.4)
//!   64..255  expansion slots (Phase 9.5.5+)
//!   256+     user-allocated

pub const USER_EID_START: u64 = 256;

// -- Entities -----------------------------------------------------------------

pub const NIL: u64 = 0;

// -- Kernel vtables (1..15) ---------------------------------------------------

pub const VTABLE_VT: u64 = 1;
pub const OBJECT_VT: u64 = 2;
pub const SELECTOR_VT: u64 = 3;
pub const CLOSURE_VT: u64 = 4;
pub const VECTOR_VT: u64 = 5;
pub const ASSOC_VT: u64 = 6;
pub const NIL_VT: u64 = 7;

/// Bootstrap module — owns the `m` column of every fact asserted during
/// kernel bootstrap. User-loaded modules get user-range EIDs.
pub const M_BOOTSTRAP: u64 = 8;

/// RRB-Tree wrapper vtable. Kernel reserves the EID and pre-materializes
/// the struct in bootstrap; dylibs install methods (count, at:) on it
/// so collect-aggregate results behave like normal sendable objects.
pub const RRB_VT: u64 = 9;

/// Phase 9: boxed f64 (full-precision float). Pointer-tagged Ref points
/// at a struct whose slot 0 holds the raw bit-pattern of the double.
pub const FLOAT_VT: u64 = 10;

/// Phase 9: boxed foreign (C) pointer. Slot 0 holds the raw address as
/// a usize. Distinct from kernel pointer Refs (which carry a -1 vt slot
/// to a kernel object) so dispatch never tries to read -1 from a foreign
/// address.
pub const FFI_PTR_VT: u64 = 11;

/// Phase 9.5: prototype vtable for loaded FFI Modules. Each `Module load:`
/// mints a fresh per-instance delegate-vt that delegates to MODULE_VT;
/// instance wraps live on the delegate-vt, shared methods (path, handle,
/// wrap:as:sig:) on MODULE_VT itself.
pub const MODULE_VT: u64 = 12;

/// Phase 9.5: diagnostic-entity vtable. Failure paths (Module load: missing
/// file, wrap: bad sig, FFI stale pointer) return a fresh entity with
/// vtable = ERROR_VT plus `:error-kind` and contextual facts. Discriminated
/// by `[res vtable] == mkEid(ERROR_VT)`.
pub const ERROR_VT: u64 = 13;

// 14..15 reserved for future kernel vtables / well-known entities

// -- Kernel selectors / attributes (16..63) -----------------------------------

pub const SEL_LOOKUP: u64 = 16;
pub const SEL_METHOD_AT_PUT_WITH: u64 = 17;
pub const SEL_DELEGATED: u64 = 18;

/// `:vtable` attribute. Value is the entity's vtable EID. Used by bind's
/// cold-path dispatch on EID-tagged receivers. Distinct from `SEL_VTABLE`
/// (the message selector "vtable" — see below) so method installs on the
/// `:vtable` selector don't collide with `:vtable` facts.
pub const ATTR_VTABLE: u64 = 19;
pub const SEL_INIT: u64 = 20;
pub const SEL_ALLOCATE: u64 = 21;

/// `:delegate` attribute on a vtable entity. Value is the parent vtable EID.
pub const ATTR_DELEGATE: u64 = 22;
/// `:name` attribute, conventional human-readable label.
pub const ATTR_NAME: u64 = 23;
/// `:module` — appears in the `m` column of every fact (see VISION.md §Fact).
pub const ATTR_MODULE: u64 = 24;

/// `vtable` message selector — returns the receiver's vtable when sent.
/// EID kept distinct from ATTR_VTABLE so methods on this selector don't
/// collide with the attribute fact.
pub const SEL_VTABLE: u64 = 25;

/// Saved-query infrastructure (Phase 7.3). A saved query is an entity with:
///   [saved_eid :saved-query-name <interned-name-EID-Ref>]
///   [saved_eid :saved-query-spec <pointer-Ref-to-*Query>]
pub const ATTR_SAVED_QUERY_NAME: u64 = 26;
pub const ATTR_SAVED_QUERY_SPEC: u64 = 27;

/// Rule storage walk-back (Phase 7.3.7). Rules are fact-entities:
///   [rule_eid :rule-name <interned-name-EID-Ref>]
///   [rule_eid :rule-params <SmallInt n_params>]
///   [rule_eid :rule-max-var <SmallInt max_var>]
///   [rule_eid :rule-body <pointer Ref to []const Clause>]
pub const ATTR_RULE_NAME: u64 = 28;
pub const ATTR_RULE_PARAMS: u64 = 29;
pub const ATTR_RULE_MAX_VAR: u64 = 30;
pub const ATTR_RULE_BODY: u64 = 31;

/// Phase 7.4: predicates + functions as fact-shaped entities.
///   [pred_eid :predicate-name <interned-name-EID-Ref>]
///   [pred_eid :predicate-impl <pointer-Ref to native NativePredBox, OR EID-Ref to closure>]
/// Same shape for functions with ATTR_FN_*. Native pointer Refs are
/// distinguished from closure Refs by tag at resolve time.
pub const ATTR_PRED_NAME: u64 = 32;
pub const ATTR_PRED_IMPL: u64 = 33;
pub const ATTR_FN_NAME: u64 = 34;
pub const ATTR_FN_IMPL: u64 = 35;

/// Phase 8: promotion threshold. Asserted on M_BOOTSTRAP at boot:
///   [M_BOOTSTRAP :promote-threshold <SmallInt N>]
/// Kernel caches the value; re-reads on mutate. Below threshold = cold,
/// at-or-above = auto-promote on next cold dispatch.
pub const ATTR_PROMOTE_THRESHOLD: u64 = 36;

/// Phase 7.5: metacircular bind override.
///   `:bind` attribute on a vtable carries the closure-EID Ref of an
///   override method. `bind` consults the delegate chain for this attribute
///   BEFORE falling through to the Zig primitive. Override returns the
///   target closure-EID Ref, or NIL to defer to the primitive.
///   ATTR_BIND vs SEL_BIND mirrors the ATTR_VTABLE / SEL_VTABLE split so
///   sending `bind` to a vtable (introspection) doesn't collide with
///   installing an override.
pub const ATTR_BIND: u64 = 37;
pub const SEL_BIND: u64 = 38;

/// Phase 9: FFI wrap-entity facts.
///   [wrap_eid :ffi-addr <FfiPtr boxed Ref>]      — symbol address
///   [wrap_eid :ffi-sig  <pointer Ref to Sig>]   — parsed dyncall sig
/// Closures registered via `ffi.registerWrap` carry `.data = mkEid(wrap_eid)`
/// and dispatch through a per-arity shim that reads these facts.
pub const ATTR_FFI_ADDR: u64 = 39;
pub const ATTR_FFI_SIG: u64 = 40;

// -- Phase 9.5: Module wrapper attributes ------------------------------------

/// `[mod :ffi-handle <FfiPtr Ref>]` — raw dlopen handle for a loaded Module.
pub const ATTR_FFI_HANDLE: u64 = 41;
/// `[mod :ffi-path <boxed string Ref>]` — filesystem path the Module was loaded from.
pub const ATTR_FFI_PATH: u64 = 42;

// -- Phase 9.5: diagnostic-entity attributes (Phase 7.8 pattern) -------------

/// `:error-kind <Ref-to-interned-EID>` — short symbolic kind name.
pub const ATTR_ERROR_KIND: u64 = 43;
/// `:error-message <boxed string Ref>` — human-readable description.
pub const ATTR_ERROR_MESSAGE: u64 = 44;
/// `:error-path <boxed string Ref>` — file path context (ffi-load-failure).
pub const ATTR_ERROR_PATH: u64 = 45;
/// `:error-name <boxed string Ref>` — symbol name context (ffi-symbol-not-found).
pub const ATTR_ERROR_NAME: u64 = 46;
/// `:error-arity <SmallInt>` — arity context (ffi-arity-too-high).
pub const ATTR_ERROR_ARITY: u64 = 47;
/// `:error-detail <boxed string Ref>` — additional detail (sig-parse-failure).
pub const ATTR_ERROR_DETAIL: u64 = 48;

// -- Phase 9.5: Module selectors --------------------------------------------

pub const SEL_LOAD: u64 = 49;
pub const SEL_WRAP_AS_SIG: u64 = 50;
pub const SEL_PATH: u64 = 51;
pub const SEL_HANDLE: u64 = 52;

/// Phase 9.5.2: schema cardinality. `[attr :cardinality :many]` opts an
/// attribute into multi-value semantics; absence (or `:one`) keeps the
/// Phase 0-9 implicit-retract-on-overwrite default.
pub const ATTR_CARDINALITY: u64 = 53;

/// Phase 9.5.3: lazy symbol enumeration. After `[mod symbols]` is sent,
/// the Mach-O reader walks the lib once and asserts one symbol entity
/// per nlist with `:ffi-symbol-name/-addr/-kind/-external?/-module` facts.
/// `:ffi-has-symbol` is :many cardinality on the Module entity itself.
/// `:ffi-symbols-enumerated?` caches the one-shot enumeration.
pub const ATTR_FFI_HAS_SYMBOL: u64 = 54;
pub const ATTR_FFI_SYMBOL_NAME: u64 = 55;
pub const ATTR_FFI_SYMBOL_ADDR: u64 = 56;
pub const ATTR_FFI_SYMBOL_KIND: u64 = 57;
pub const ATTR_FFI_SYMBOL_EXTERNAL: u64 = 58;
pub const ATTR_FFI_SYMBOL_MODULE: u64 = 59;
pub const ATTR_FFI_SYMBOLS_ENUMERATED: u64 = 60;
pub const SEL_SYMBOLS: u64 = 61;

/// Phase 9.5.4: fact-dispatched binary-format reader registry per
/// decision-doc B2. Each reader self-registers at boot:
///   [reader_eid :reader-magic <SmallInt> :reader-impl <fn-ptr Ref>]
/// `loader.openSymbols` sniffs the first 4 bytes of an input buffer and
/// dispatches via fact query, so adding a new format (PE, COFF, etc.) is
/// a fact assertion — no hardcoded switch in the kernel.
pub const ATTR_READER_MAGIC: u64 = 62;
pub const ATTR_READER_IMPL: u64 = 63;

/// Phase 9.5.5: Module unload + stale-pointer machinery.
///   [ffi_ptr_obj :ffi-owner <mod_eid>]  — every boxed FFI ptr knows
///                                         which Module minted it.
///   [ffi_ptr_obj :ffi-stale? :true]    — set on unload; pushArg checks.
pub const ATTR_FFI_OWNER: u64 = 64;
pub const ATTR_FFI_STALE: u64 = 65;
pub const SEL_UNLOAD: u64 = 66;

// -- Phase 10.0 kernel vtables + singletons (67..76) -------------------------

/// `BLOCK_VT` — vtable for block closures (Phase 10.3 evaluator). User
/// `[:x | body]` literals materialize as closure entities with this vtable.
pub const BLOCK_VT: u64 = 67;

/// `SMALLINT_VT` — vtable for immediate-tag SmallInts. `send.vtableEidOf`
/// resolves any SmallInt tag to this EID so arithmetic methods install
/// uniformly via the standard `[vt sel closure]` shape.
pub const SMALLINT_VT: u64 = 68;

/// `BOOLEAN_VT` — common-delegate prototype for True / False. Carries
/// default `not`, `and:`, `or:` impls in terms of `ifTrue:ifFalse:`.
/// True / False each `:delegate` to BOOLEAN_VT.
pub const BOOLEAN_VT: u64 = 69;

/// `TRUE_VT` — prototype-vt for the true singleton. Overrides
/// `ifTrue:ifFalse:` (+ `ifTrue:`, `ifFalse:`).
pub const TRUE_VT: u64 = 70;

/// `FALSE_VT` — prototype-vt for the false singleton.
pub const FALSE_VT: u64 = 71;

/// `TRUE_EID` / `FALSE_EID` — the true / false singleton instances.
/// Distinct EIDs from their vtables (matches NIL / NIL_VT precedent).
/// `[TRUE_EID :vtable TRUE_VT]` asserted at boot; same for FALSE_EID.
pub const TRUE_EID: u64 = 72;
pub const FALSE_EID: u64 = 73;

/// `GRAMMAR_VT` — prototype-vt for PEG grammars (Phase 10.0). Each grammar
/// is an entity carrying a linked-list of rule entities.
pub const GRAMMAR_VT: u64 = 74;

/// `PEG_VT` — module-singleton vtable for the PEG engine façade. Holds the
/// send-dispatched API surface (`[Grammar new]`, etc.), bound at PEG dylib
/// load (10.0) / 10.1 install sequence.
pub const PEG_VT: u64 = 75;

/// `M_PEG` — module-provenance entity for the PEG dylib. Used as the `m`
/// column on every fact asserted by the PEG engine.
pub const M_PEG: u64 = 76;

// -- Phase 10.0 PEG pattern + grammar attributes (77..88) -------------------

/// `[pat :pattern-kind <interned-keyword>]` — combinator tag for a pattern
/// entity. Values: `literal`, `seq`, `choice`, `star`, `plus`, `opt`, `and`,
/// `not`, `rule-ref`, `charset`, `any`, `eof` (P7 locked set of 12).
pub const ATTR_PATTERN_KIND: u64 = 77;
/// `[pat :pattern-first-child <child_eid>]` / `:pattern-next-sibling` form
/// a linked-list of sub-patterns under a sequence / choice / unary-wrapper
/// combinator (P3 shape, matches AST child encoding).
pub const ATTR_PATTERN_FIRST_CHILD: u64 = 78;
pub const ATTR_PATTERN_NEXT_SIBLING: u64 = 79;
/// `[pat :literal-value <boxed-string Ref>]` — the literal byte-string a
/// `:literal` combinator matches.
pub const ATTR_LITERAL_VALUE: u64 = 80;
/// `[pat :rule-ref-name <interned-name-EID>]` — the rule a `:rule-ref`
/// pattern resolves to (looked up in the enclosing grammar at match time).
pub const ATTR_RULE_REF_NAME: u64 = 81;
/// Charset (P6): linked-list of range entities, each carrying a u8 code
/// point range. `[cs :charset-first-range <range_eid>]`,
/// `[range :charset-next-range ?]`, `[range :charset-range-lo <SmallInt>]`,
/// `[range :charset-range-hi <SmallInt>]`.
pub const ATTR_CHARSET_FIRST_RANGE: u64 = 82;
pub const ATTR_CHARSET_NEXT_RANGE: u64 = 83;
pub const ATTR_CHARSET_RANGE_LO: u64 = 84;
pub const ATTR_CHARSET_RANGE_HI: u64 = 85;
/// `[grammar :grammar-first-rule <rule_eid>]` — head of grammar's rule list.
/// `[rule :rule-next <rule_eid>]` chains rules within a grammar.
/// `[rule :rule-pattern <pat_eid>]` points at the rule's root pattern.
/// Rule entities reuse `ATTR_RULE_NAME` (already at 28, Phase 7.3) — disjoint
/// attribute sets with datalog rules, no semantic collision.
pub const ATTR_GRAMMAR_FIRST_RULE: u64 = 86;
pub const ATTR_RULE_NEXT: u64 = 87;
pub const ATTR_RULE_PATTERN: u64 = 88;

// -- Phase 10.0 PEG selectors (89..90) --------------------------------------

pub const SEL_DEFINE_RULE_AS: u64 = 89;
pub const SEL_PARSE_FROM: u64 = 90;

// -- Phase 10.0 generic AST / CST attributes (91..98) -----------------------

/// `[ast :ast-rule-name <interned-name-EID>]` — for CST nodes produced by
/// a rule-ref match, the name of the rule that produced this node. NIL
/// for anonymous sub-patterns (seq / choice / star / etc. don't emit CST
/// nodes, only their containing rule does).
pub const ATTR_AST_RULE_NAME: u64 = 91;
/// AST children encoded as a singly-linked list via these two attrs (P3).
pub const ATTR_AST_FIRST_CHILD: u64 = 92;
pub const ATTR_AST_NEXT_SIBLING: u64 = 93;
/// `[ast :ast-byte-start <SmallInt>]` / `:ast-byte-end` — byte span into
/// the parse-time source string (P4). Line/col derived on demand via the
/// `lineCol(source, offset)` helper at the lang layer.
pub const ATTR_AST_BYTE_START: u64 = 94;
pub const ATTR_AST_BYTE_END: u64 = 95;
/// `[ast :ast-parse <parse_eid>]` — back-reference to the enclosing parse
/// entity for Phase 13.4 mark-sweep grouping. Permitted to be NIL when
/// the engine is invoked without a parse-bracketing entity (e.g. tests).
pub const ATTR_AST_PARSE: u64 = 96;
/// Parse failure diagnostic — `[err :peg-expected <interned-name-EID>]`
/// asserted with `:many` cardinality (one fact per expected rule/literal
/// at the furthest-failure position). `:peg-position` carries the byte
/// offset (SmallInt).
pub const ATTR_PEG_EXPECTED: u64 = 97;
pub const ATTR_PEG_POSITION: u64 = 98;

// 99..255 reserved for future kernel selectors / attributes

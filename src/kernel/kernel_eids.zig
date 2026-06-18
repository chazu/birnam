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

/// Phase 9.5.6: the `Module` prototype object. `MODULE_PROTO` is a singleton
/// object (birnam has prototypes, not classes) whose vtable (`MODULE_PROTO_VT`)
/// carries `load:` (→ `ffi_module.moduleLoad`) and `resolve` (→ resolve all
/// pending modules). The lang frontend binds the global name `Module` to
/// `mkEid(MODULE_PROTO)` so `Module load: "lib.dylib"` is reachable from
/// REPL/source. Sending `load:` mints a NEW module object — not class
/// instantiation. Distinct from `MODULE_VT` (per-module instance side:
/// path/handle/symbols/wrap:as:signature:/unload).
pub const MODULE_PROTO_VT: u64 = 14;
pub const MODULE_PROTO: u64 = 15;

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

/// `PEG_EID` — singleton instance entity for the PEG factory namespace.
/// `[PEG_EID :vtable PEG_VT]`; pattern-constructor send-dispatched methods
/// (`literal:`, `ruleRef:`, `star:`, …) live on PEG_VT. The Phase 10.1
/// lang dylib binds `Pattern` (and `Grammar`) to this singleton in the
/// top-level scope per MD2.
pub const PEG_EID: u64 = 99;

// -- Phase 10.1 — language module kernel slots (100..161) -------------------

// Binding + scope (ADR-10-E2 truth layer).
pub const BINDING_VT: u64 = 100;
pub const SCOPE_VT: u64 = 101;

// Lang module provenance + singleton.
pub const M_LANG: u64 = 102;
pub const LANG_VT: u64 = 103;
pub const LANG_EID: u64 = 104;

// AST kind vtables (E1 — per-kind eval dispatch).
/// Generic literal — `:ast-value` carries the immediate Ref (SmallInt,
/// boxed Float, string, symbol EID, pre-built Vector Ref, NIL, TRUE_EID,
/// FALSE_EID). Eval = return `:ast-value`.
pub const AST_LITERAL_VT: u64 = 105;
pub const AST_VAR_REF_VT: u64 = 106;
pub const AST_SELF_VT: u64 = 107;
pub const AST_KEYWORD_SEND_VT: u64 = 108;
pub const AST_UNARY_SEND_VT: u64 = 109;
pub const AST_BINARY_SEND_VT: u64 = 110;
pub const AST_PATTERN_SEND_VT: u64 = 111;
pub const AST_BLOCK_VT: u64 = 112;
pub const AST_LET_VT: u64 = 113;
pub const AST_ASSIGN_VT: u64 = 114;
pub const AST_RETURN_VT: u64 = 115;
pub const AST_SUPER_SEND_VT: u64 = 116;
pub const AST_STMT_SEQ_VT: u64 = 117;

// AST attributes (Phase 10.1 — extends Phase 10.0's generic :ast-* set).
pub const ATTR_AST_VALUE: u64 = 118;
pub const ATTR_AST_NAME: u64 = 119;
pub const ATTR_AST_RECEIVER: u64 = 120;
pub const ATTR_AST_SELECTOR: u64 = 121;
pub const ATTR_AST_FIRST_ARG: u64 = 122;
pub const ATTR_AST_BLOCK_PARAMS: u64 = 123;
pub const ATTR_AST_BLOCK_BODY: u64 = 124;
pub const ATTR_AST_LET_INIT: u64 = 125;
pub const ATTR_AST_ASSIGN_TARGET: u64 = 126;
pub const ATTR_AST_ASSIGN_VALUE: u64 = 127;
pub const ATTR_AST_RETURN_VALUE: u64 = 128;
/// Parse-time slot resolution (ADR-10-E2 speed layer).
pub const ATTR_AST_BIND_DEPTH: u64 = 129;
pub const ATTR_AST_BIND_SLOT: u64 = 130;
pub const ATTR_AST_BIND_NAME: u64 = 131;
/// Parse-time enclosing-method back-pointer on `^` AST nodes.
pub const ATTR_AST_METHOD_FRAME: u64 = 132;
/// Parse-time enclosing-method vt on `[super …]` AST nodes (MD4).
pub const ATTR_AST_SUPER_VT: u64 = 133;
pub const ATTR_AST_STMT_FIRST: u64 = 134;

// Binding + scope attrs.
pub const ATTR_BIND_SCOPE: u64 = 135;
pub const ATTR_BIND_NAME: u64 = 136;
pub const ATTR_BIND_VALUE: u64 = 137;
pub const ATTR_SCOPE_PARENT: u64 = 138;
pub const ATTR_SCOPE_TOP_LEVEL: u64 = 139;

// Selectors (control flow + arithmetic + booleans + block iteration).
pub const SEL_VALUE: u64 = 140;
pub const SEL_VALUE_OF: u64 = 141; // "value:"
pub const SEL_IF_TRUE: u64 = 142;
pub const SEL_IF_FALSE: u64 = 143;
pub const SEL_IF_TRUE_IF_FALSE: u64 = 144;
pub const SEL_WHILE_TRUE: u64 = 145;
pub const SEL_WHILE_FALSE: u64 = 146;
pub const SEL_TIMES_REPEAT: u64 = 147;
pub const SEL_TO_DO: u64 = 148;
pub const SEL_NOT: u64 = 149;
pub const SEL_AND: u64 = 150;
pub const SEL_OR: u64 = 151;
pub const SEL_PLUS: u64 = 152;
pub const SEL_MINUS: u64 = 153;
pub const SEL_TIMES: u64 = 154;
pub const SEL_DIVIDE: u64 = 155;
pub const SEL_MOD: u64 = 156;
pub const SEL_EQ: u64 = 157;
pub const SEL_LT: u64 = 158;
pub const SEL_GT: u64 = 159;
pub const SEL_LE: u64 = 160;
pub const SEL_GE: u64 = 161;

// -- Phase 10.4.5 ADR-10-E2 scope chain (162..164) --------------------------

/// `[scope :scope-self <receiver Ref>]` — current `self` for the activation
/// behind this scope. AST_SELF_VT eval queries this attribute on `args[0]`
/// (the current scope-Ref threaded through every eval method).
pub const ATTR_SCOPE_SELF: u64 = 162;
/// `[scope :scope-first-binding <BINDING_VT eid>]` — head of the scope's
/// binding linked-list. Bindings chain via `:bind-next`. AST_VAR_REF_VT eval
/// walks this list, then walks `:scope-parent` if no match.
pub const ATTR_SCOPE_FIRST_BINDING: u64 = 163;
/// `[bind :bind-next <BINDING_VT eid>]` — linked-list tail pointer between
/// bindings in a single scope.
pub const ATTR_BIND_NEXT: u64 = 164;

// -- Phase 11 — bytecode VM (165..182) --------------------------------------

/// `[method :bytecode-stream <byte-array-Ref>]` — pointer-Ref to a
/// materialized byte slice in the bytecode arena. ADR-11 #1.
pub const ATTR_BYTECODE_STREAM: u64 = 165;
/// `[method :constant-pool <vec-Ref>]` — RRB-Tree Ref to vec of Refs
/// (number/string literal entities, block-method Refs, symbol entities).
/// Selectors NOT in pool — inline u32 in SEND_* opcodes per ADR-11 addendum.
pub const ATTR_CONSTANT_POOL: u64 = 166;
/// `[method :method-body-ast <ast-eid>]` — provenance back-pointer to the
/// AST root the compiler lowered. Phase 13.4 mark-sweep root. ADR-11
/// addendum reuses this name (drops the original `:source-ast` synonym).
pub const ATTR_METHOD_BODY_AST: u64 = 167;
/// `[method :method-sends ?sel-eid]` cardinality :many — every selector
/// this method statically sends. Tooling/queryable-compiler annotation.
pub const ATTR_METHOD_SENDS: u64 = 168;
/// `[method :method-reads-slot ?slot-eid]` cardinality :many.
pub const ATTR_METHOD_READS_SLOT: u64 = 169;
/// `[method :method-ast-range ?range-eid]` cardinality :many. Each range
/// entity carries :range-method / :range-pc-start / :range-pc-end /
/// :range-ast (ADR-11 addendum, Date 1NF over the 3-tuple).
pub const ATTR_METHOD_AST_RANGE: u64 = 170;
/// `[method :method-send-site-count <SmallInt>]` — drives mcache sizing
/// on first execution (lazy alloc per ADR-11 addendum #5).
pub const ATTR_METHOD_SEND_SITE_COUNT: u64 = 171;
/// `[method :method-local-count <SmallInt>]` — frame locals_inline size.
pub const ATTR_METHOD_LOCAL_COUNT: u64 = 172;
/// `[method :method-arg-count <SmallInt>]` — caller arg-pop count after
/// SEND_N (SEND_0/SEND_1 bake arg count into opcode).
pub const ATTR_METHOD_ARG_COUNT: u64 = 173;
/// `[block-method :block-parent-method <method-eid>]` — enclosing method
/// for RETURN_FROM_METHOD static depth resolution. ADR-11 addendum #1
/// (block = own bytecode-method entity).
pub const ATTR_BLOCK_PARENT_METHOD: u64 = 174;

/// Range-entity attributes — sub-entity that encodes (pc-start, pc-end,
/// ast) per Date 1NF (ADR-11 addendum #2).
pub const ATTR_RANGE_METHOD: u64 = 175;
pub const ATTR_RANGE_PC_START: u64 = 176;
pub const ATTR_RANGE_PC_END: u64 = 177;
pub const ATTR_RANGE_AST: u64 = 178;

// -- Phase 11.6 — reify-stack frame entities (179..182) ----------------------

/// `[frame :frame-pc <SmallInt>]` — program counter at the moment of reify.
pub const ATTR_FRAME_PC: u64 = 179;
/// `[frame :frame-method <method-eid>]` — currently-executing method.
pub const ATTR_FRAME_METHOD: u64 = 180;
/// `[frame :frame-self <Ref>]` — receiver of the current activation.
pub const ATTR_FRAME_SELF: u64 = 181;
/// `[frame :frame-parent <frame-eid>]` — calling frame in the reified chain.
pub const ATTR_FRAME_PARENT: u64 = 182;

// -- Phase 11.11 — closure scope capture (183) -------------------------------

/// `[method :method-captures-ptr <pointer-Ref>]` — single-value pointer to
/// a heap-allocated CaptureSpecArray (extern struct `{ len: u32, _pad,
/// [len]CaptureSpec }`). Each CaptureSpec carries `(parent_slot,
/// local_slot)` telling MAKE_BLOCK which parent-frame local to read and
/// runMethod which child-frame slot to write the captured value into.
/// Omitted (Date 1NF) when the method captures nothing.
pub const ATTR_METHOD_CAPTURES_PTR: u64 = 183;

// -- Phase 12.0 — provenance split (184) -------------------------------------

/// `M_USER` — module-provenance tag for runtime-created DATA facts (parsed
/// AST, scope/binding entities, user-installed methods, REPL mutations).
/// Distinct from any code-module eid (bootstrap, the dlopen-assigned module
/// eids, `M_LANG`/`M_PEG`): CODE facts are reconstructed every boot by
/// re-running module init, so they ride the unstable runtime module eid;
/// DATA facts must ride this stable well-known eid so the image snapshot
/// (Phase 12) can find them and so module unload can't retract them. The
/// image writer persists `m == M_USER` facts only.
pub const M_USER: u64 = 184;

/// `TOP_SCOPE_EID` — stable well-known eid for the lang module's top-level
/// REPL scope. Phase 12: REPL bindings are persisted M_USER data and hang
/// off this scope (`[TOP_SCOPE :scope-first-binding ...]`). The scope entity
/// itself is reconstructed each boot by lang init (CODE), but at this fixed
/// eid — so reloaded bindings re-attach instead of dangling on a
/// freshly-allocated scope eid.
pub const TOP_SCOPE_EID: u64 = 185;

/// Phase 9.5.6 — relinkable-wrap schema. An FFI wrap is a DECLARATIVE entity;
/// the side-effecting `dlopen`/`dlsym` is deferred to `resolveModule`. Durable
/// (M_USER, persisted in images) facts on a wrap entity:
///   [wrap :ffi-owner <module_eid>]        (ATTR_FFI_OWNER — already defined)
///   [wrap :ffi-wrap-symbol "foo_open"]    symbol name → re-`dlsym`able
///   [wrap :ffi-sig-str "p)p"]             dyncall sig string → re-parseable
///   [wrap :wrap-selector #open:]          where to install on the module vt
/// The transient resolved facts ([wrap :ffi-addr <ptr>], [wrap :ffi-sig <Sig*>],
/// [module :ffi-handle <dlopen-handle>]) are M_BOOTSTRAP and rebuilt every process by
/// `resolveModule` — so persistence/freeze relinks by NAME, never by address.
pub const ATTR_FFI_WRAP_SYMBOL: u64 = 187;
pub const ATTR_FFI_SIG_STR: u64 = 188;
pub const ATTR_WRAP_SELECTOR: u64 = 189;

/// `resolve` selector on `MODULE_PROTO_VT` — `dlopen`s every pending module and
/// `dlsym`s its declarative wraps, installing dispatch closures. Idempotent;
/// run at REPL line-commit, file-in commit, and image load.
pub const SEL_RESOLVE: u64 = 190;

/// Phase 9.5.6 — file-in. The `language` prototype object (lang owns the
/// methods: `fileIn:` / `fileIn:onError:`); kernel only reserves the well-known
/// eids so they survive the eid allocator. A filed-in unit is recorded as an
/// entity `[unit :filed-in-path "p"] [unit :filed-in-source "..."]` (M_USER) so
/// the wrapper stays re-editable / re-emittable (freeze).
pub const LANGUAGE_PROTO_VT: u64 = 191;
pub const LANGUAGE_PROTO: u64 = 192;
pub const ATTR_FILED_IN_PATH: u64 = 193;
pub const ATTR_FILED_IN_SOURCE: u64 = 194;

/// `GLOBALS_SCOPE_EID` — stable well-known eid for the lang module's globals
/// scope: the *parent* of `TOP_SCOPE_EID`. Built-in global bindings (e.g.
/// `Module`) live here, not in the top scope, so that `:reset` (which clears
/// only TOP_SCOPE's binding chain) and image `:load` (which replays the
/// persisted M_USER TOP_SCOPE first-binding pointer) leave globals intact.
/// Reconstructed each boot by lang init as CODE; never persisted.
pub const GLOBALS_SCOPE_EID: u64 = 186;

// -- Phase 15.R R1d: db-value language surface -------------------------------
/// `SNAPSHOT_VT` — vtable of a first-class snapshot object. The object is a
/// materialized `[vt][snap_ptr]` libid pair (vt at -1, like a closure) whose
/// `snap_ptr` slot holds a raw pointer to a kernel `FactStore.Snapshot`.
pub const SNAPSHOT_VT: u64 = 195;
/// `DB_PROTO_VT` — vtable carrying the `db` prototype's methods (now / asOf: /
/// retain: / excise:).
pub const DB_PROTO_VT: u64 = 196;
/// `DB_PROTO` — the singleton `db` prototype object, bound to the global `db`.
pub const DB_PROTO: u64 = 197;

/// `doesNotUnderstand:` selector — sent to a receiver when dispatch finds no
/// method for a selector (Phase 16 Tier D). Seeded at boot so `send.zig` (which
/// can't intern) can reference it. The handler receives the missing selector as
/// a symbol Ref.
pub const SEL_DNU: u64 = 198;

/// `REDUCED_VT` — vtable of a `reduced` early-out sentinel (Phase 16): a 1-slot
/// box wrapping the final fold value; `inject:into:` stops when it sees one.
pub const REDUCED_VT: u64 = 199;
/// `RANGE_VT` — vtable of an integer range value (`[lo to: hi]` / `to:by:`),
/// a lazy seq answering the `do:`/`inject:into:` protocol (Phase 16).
pub const RANGE_VT: u64 = 200;
/// `STRING_VT` — vtable for tag-100 string Refs. Dispatch (`vtableEidOf`)
/// returns it for any string so strings answer messages (Phase 16).
pub const STRING_VT: u64 = 201;

/// `CELL_VT` — vtable for reference cells (Phase 16 Tier E). A cell IS an
/// entity: `[cell :vtable CELL_VT] [cell :cell-value <v>]`. Reads
/// (`value`) = `currentValue`; writes (`set:`) = `assert` (new tx). Time
/// travel (`asOf:`/`history`) rides 15.R per-cell history for free. `cell:`
/// / `ref:` / `atom:` on the `db` prototype all mint one (single-transactor
/// subsumes Clojure's atom/ref/agent distinction).
pub const CELL_VT: u64 = 202;
/// `:cell-value` — the one attribute a cell entity carries.
pub const ATTR_CELL_VALUE: u64 = 203;

/// `:meta` — metadata attribute (Phase 16 Tier E). `withMeta:` asserts it,
/// `meta` reads it. Free: just another fact; equality already ignores it.
pub const ATTR_META: u64 = 204;

/// `XF_VT` / `XF_PROTO` — the transducer factory (Phase 16 Tier E). `Xf` is a
/// singleton prototype whose vtable carries `map:`/`filter:`/`take:`
/// constructors (each returns a transducer closure). A transducer is an
/// rf→rf' block; `coll transduce: xf` runs it eagerly in one fold (no
/// intermediate vectors). `comp` is deferred per ROADMAP DNA — composition
/// rides `aXf then: bXf` on CLOSURE_VT until channels justify `comp`.
pub const XF_VT: u64 = 205;
pub const XF_PROTO: u64 = 206;

/// Phase 18 prelude — FFI memory primitives. `alloc:` lives on MODULE_PROTO_VT
/// (`Module alloc: n` → FfiPtr to `n` zeroed libc bytes). The rest are methods
/// on FFI_PTR_VT: scalar/pointer/cstring peek+poke through a raw address (byte
/// offsets), plus `free`/`address`/`isNull`. These make out-params (alloc a
/// cell, pass its FfiPtr, peek it back) and opaque-pointer deref possible from
/// the REPL with no shim.
pub const SEL_FFI_ALLOC: u64 = 207; // "alloc:"        on MODULE_PROTO_VT
pub const SEL_FFI_FREE: u64 = 208; // "free"
pub const SEL_FFI_ADDRESS: u64 = 209; // "address"
pub const SEL_FFI_IS_NULL: u64 = 210; // "isNull"
pub const SEL_FFI_PEEK_INT: u64 = 211; // "peekInt:"     (offset) -> i64 (8 bytes)
pub const SEL_FFI_PEEK_BYTE: u64 = 212; // "peekByte:"    (offset) -> i64 (0..255)
pub const SEL_FFI_PEEK_DOUBLE: u64 = 213; // "peekDouble:" (offset) -> Float
pub const SEL_FFI_PEEK_PTR: u64 = 214; // "peekPtr:"     (offset) -> FfiPtr
pub const SEL_FFI_PEEK_STRING: u64 = 215; // "peekString:" (offset) -> string
pub const SEL_FFI_POKE_INT: u64 = 216; // "pokeInt:at:"   (value, offset) -> self
pub const SEL_FFI_POKE_BYTE: u64 = 217; // "pokeByte:at:"  (value, offset) -> self
pub const SEL_FFI_POKE_DOUBLE: u64 = 218; // "pokeDouble:at:" (value, offset) -> self
pub const SEL_FFI_POKE_PTR: u64 = 219; // "pokePtr:at:"   (ptr, offset) -> self

/// Phase 17.8 — I/O surface. `File` and `System` prototype singletons (like
/// `Module`/`db`), installed lang-side. File does data read/write (returns
/// ERROR_VT on failure); System does stdout/stderr/stdin/argv.
pub const FILE_PROTO_VT: u64 = 220;
pub const FILE_PROTO: u64 = 221;
pub const SYSTEM_PROTO_VT: u64 = 222;
pub const SYSTEM_PROTO: u64 = 223;

/// Phase 19.3 — `char**` string arrays. `argv:` builds a single-block,
/// NULL-terminated `char**` from a birnam vector of strings (freed via one
/// `[p free]`); `peekStringArray:` reads a `char**` back into a vector (a
/// SmallInt count reads exactly N entries, `nil` reads until the NULL terminator).
pub const SEL_FFI_ARGV: u64 = 224; // "argv:"            on MODULE_PROTO_VT
pub const SEL_FFI_PEEK_STRING_ARRAY: u64 = 225; // "peekStringArray:" on FFI_PTR_VT

/// Phase 19.4 — bulk byte-buffer copy between an FfiPtr and a birnam string.
/// `peekBytes:len:` copies `len` bytes at `base + offset` into a new string;
/// `pokeBytes:at:` memcpys a string's (or byte-vector's) bytes to `base +
/// offset`. Length/offset are caller-trusted (a raw FfiPtr carries no bounds).
pub const SEL_FFI_PEEK_BYTES: u64 = 226; // "peekBytes:len:" (offset, len) -> string
pub const SEL_FFI_POKE_BYTES: u64 = 227; // "pokeBytes:at:"  (bytes, offset) -> self

// Width-complete peek/poke family (2026-06-17). `peekInt:`/`pokeInt:at:` are
// 8-byte (C `long`/`int64`); these add narrower C scalar widths so a 4-byte C
// `int` (e.g. errno) or 2-byte `short` (e.g. `in_port_t`) reads back as a
// SmallInt without the 8-byte read overrunning adjacent bytes (which then
// overflowed the 61-bit SmallInt → `nil`). Signed variants sign-extend; `U`
// variants zero-extend (always fit 61-bit). Poke truncates the low bytes, so
// signedness is irrelevant on write (one selector per width). EIDs 240-245 sit
// below USER_EID_START (256); 246-255 remain free.
pub const SEL_FFI_PEEK_INT32: u64 = 240; // "peekInt32:"  (offset) -> i32 sign-extended
pub const SEL_FFI_PEEK_UINT32: u64 = 241; // "peekUInt32:" (offset) -> u32 zero-extended
pub const SEL_FFI_PEEK_SHORT: u64 = 242; // "peekShort:"  (offset) -> i16 sign-extended
pub const SEL_FFI_PEEK_USHORT: u64 = 243; // "peekUShort:" (offset) -> u16 zero-extended
pub const SEL_FFI_POKE_INT32: u64 = 244; // "pokeInt32:at:" (value, offset) -> self (low 4 bytes)
pub const SEL_FFI_POKE_SHORT: u64 = 245; // "pokeShort:at:" (value, offset) -> self (low 2 bytes)

/// Phase 19.7 — out-param "cell" constructors on MODULE_PROTO_VT. Each mints an
/// FfiPtr to a fresh scratch buffer carrying a width tag (boxed slot 2); after a
/// C call writes through it, `[cell value]` (reuses SEL_VALUE) peeks the right
/// width and `[cell free]` releases it. Collapses the alloc/pass/peek/free
/// out-param dance (e.g. `open(path, &fd)`) into one minted value.
pub const SEL_FFI_OUT_INT: u64 = 228; // "outInt"    -> cell over a C int (4 bytes)
pub const SEL_FFI_OUT_LONG: u64 = 229; // "outLong"  -> cell over a C long (8 bytes)
pub const SEL_FFI_OUT_PTR: u64 = 230; // "outPtr"    -> cell over a pointer (8 bytes)
pub const SEL_FFI_OUT_DOUBLE: u64 = 231; // "outDouble" -> cell over a double (8 bytes)

/// Phase 19.2 — callbacks (birnam closure → C function pointer via libffi
/// `ffi_closure`). `CALLBACK_VT` is the vtable for callback objects: slot 0 = the
/// C-callable code location (so it marshals as a `p`/pointer FFI arg), slot 1 = a
/// pointer to the heap `CallbackCtx` (closure Ref + parsed sig + ffi_closure
/// handle) for invocation + teardown. `[Module callback: aBlock sig: "pp)i"]`
/// mints one; `[cb free]` (reuses SEL_FFI_FREE) releases the trampoline.
pub const CALLBACK_VT: u64 = 232;
pub const SEL_FFI_CALLBACK: u64 = 233; // "callback:sig:" on MODULE_PROTO_VT

/// Phase 19.5 — struct-as-entity. `[Module struct: #Point fields: #(#x #i #y #i)]`
/// (an ORDERED vector of name/type symbol pairs — C field order matters, maps
/// don't preserve it) registers a struct TYPE (STRUCT_TYPE_VT object; slot 0 =
/// the minted per-type instance vtable, slot 1 = byte size). `[T from: anFfiPtr]`
/// wraps existing bytes, `[T new]` mallocs a zeroed instance, `[T size]` → bytes.
/// An instance carries its backing C address in slot 0 and answers each field
/// name as a getter (`[pt x]`) / `name:` as a setter (`[pt x: 3]`) that lower to
/// typed peek/poke at the field offset; `[pt ptr]` → the backing FfiPtr (pass
/// the struct* to C), `[pt free]` (SEL_FFI_FREE) frees a `new`-minted backing.
pub const STRUCT_TYPE_VT: u64 = 234;
pub const SEL_FFI_STRUCT: u64 = 235; // "struct:fields:" on MODULE_PROTO_VT
pub const SEL_FFI_STRUCT_FROM: u64 = 236; // "from:"  on STRUCT_TYPE_VT
pub const SEL_FFI_STRUCT_NEW: u64 = 237; // "new"    on STRUCT_TYPE_VT
pub const SEL_FFI_STRUCT_PTR: u64 = 238; // "ptr"    on a struct instance

/// `BYTECODE_METHOD_VT` — vtable of a materialized bytecode METHOD entity
/// (Phase 12.8 / Approach A). A compiled method is no longer dressed as a
/// `CLOSURE_VT` closure: its object-table slot is `[BYTECODE_METHOD_VT]
/// [method=&bytecodeDispatcherImpl][data=mkEid(method_eid)]`. The `.method`
/// fn-ptr is retained ONLY as a stable discriminant + fallback (gc/prune/
/// id_api key on it, and `value`-on-a-method-eid routes through it); the hot
/// dispatch path (`send.sendN`) recognizes a bytecode method at the bind
/// boundary and enters `runMethod` DIRECTLY — no fn-ptr shim, no re-dispatch.
/// This splits the native-primitive / bytecode-method conflation at the
/// representation layer (methods emit as DATA) ahead of Phase 18 codegen.
/// Delegates to `CLOSURE_VT` so `value`/`whileTrue:`/`on:do:` still resolve
/// when a method-eid is sent like a block.
pub const BYTECODE_METHOD_VT: u64 = 239;

// Stdlib `Time` / `Random` global prototypes (Phase 17.9 — wall clock + PRNG).
// CODE provenance, rebuilt each boot like File/System; reserved so an image
// round-trip keeps their identity.
pub const TIME_PROTO_VT: u64 = 246;
pub const TIME_PROTO: u64 = 247;
pub const RANDOM_PROTO_VT: u64 = 248;
pub const RANDOM_PROTO: u64 = 249;

// 250..255 reserved for future kernel selectors / attributes

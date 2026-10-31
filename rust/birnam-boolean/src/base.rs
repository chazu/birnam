//! Boolean owns canonical identities; conditionals send only to the chosen block.
use birnam_abi::{
    self as abi, Oop,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{cell::Cell, ptr};
use std::sync::atomic::{AtomicPtr, Ordering};

struct State {
    id: Protocol,
    namespace: Oop,
    roots: Oop,
    image: Oop,
    nil: Oop,
    truth: Oop,
    falsity: Oop,
    names: [Oop; NAMES.len()],
    true_text: Oop,
    false_text: Oop,
    printing: Cell<bool>,
    ready: Cell<bool>,
}

static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
unsafe fn state<'a>(c: Oop) -> Result<&'a State, Oop> {
    let raw = INSTANCE.load(Ordering::Acquire);
    if raw.is_null() {
        let api = API.load(Ordering::Acquire);
        if api.is_null() {
            std::process::abort();
        }
        return Err(unsafe { (*api).invalid });
    }
    let s = unsafe { &*raw };
    if c == s.truth {
        Ok(s)
    } else {
        Err(s.id.invalid)
    }
}
impl State {
    unsafe fn send(&self, r: Oop, selector: usize, a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[selector], a) }
    }
    fn polarity(&self, receiver: Oop) -> Option<bool> {
        if receiver == self.truth {
            Some(true)
        } else if receiver == self.falsity {
            Some(false)
        } else {
            None
        }
    }
    unsafe fn perform(&self, op: Op, r: Oop, a: &[Oop]) -> Result<Oop, Oop> {
        use Op::*;
        unsafe {
            match (op, a) {
                (Identical, [v]) if !v.is_null() => {
                    Ok(if r == *v { self.truth } else { self.falsity })
                }
                (Not, []) => Ok(match self.polarity(r) {
                    Some(true) => self.falsity,
                    Some(false) => self.truth,
                    None => self.nil,
                }),
                (And | Or, [v]) if !v.is_null() => Ok(match (self.polarity(r), op) {
                    (Some(true), And) | (Some(false), Or) => *v,
                    (Some(_), _) => r,
                    (None, _) => self.nil,
                }),
                (IfBoth, [yes, no]) if !yes.is_null() && !no.is_null() => match self.polarity(r) {
                    Some(truth) => self.send(if truth { *yes } else { *no }, VALUE, &[]),
                    None => Ok(self.nil),
                },
                (IfTrue | IfFalse | AndBlock | OrBlock, [block]) if !block.is_null() => {
                    match (self.polarity(r), op) {
                        (Some(true), IfTrue | AndBlock) | (Some(false), IfFalse | OrBlock) => {
                            self.send(*block, VALUE, &[])
                        }
                        (Some(_), AndBlock | OrBlock) => Ok(r),
                        _ => Ok(self.nil),
                    }
                }
                (Roots, [visitor]) => {
                    for v in [
                        self.namespace,
                        self.roots,
                        self.image,
                        self.nil,
                        self.truth,
                        self.falsity,
                    ]
                    .into_iter()
                    .chain(self.names.iter().copied())
                    {
                        self.send(*visitor, VISIT, &[v])?;
                    }
                    Ok(ptr::null_mut())
                }
                (InstallPrinting, [string])
                    if r == self.truth && !string.is_null() && !self.printing.get() =>
                {
                    let behavior = self.send(self.id.root, BEHAVIOR_OF, &[r])?;
                    let selector = self.names[PRINT_STRING];
                    if self.send(behavior, LOOKUP, &[selector])?.is_null() {
                        let method =
                            self.send(self.image, METHOD_AT, &[self.names[PRINT_BOOLEAN]])?;
                        let method = self.send(method, WITH_CONTEXT, &[*string])?;
                        self.send(behavior, ADD_METHOD, &[selector, method])?;
                    }
                    self.printing.set(true);
                    Ok(ptr::null_mut())
                }
                _ => Err(self.id.invalid),
            }
        }
    }
}
macro_rules! operations {($(($name:ident,$op:ident)),* $(,)?)=>{
    #[derive(Clone,Copy)]enum Op{$($op),*}
    #[allow(non_snake_case)]mod entries{use super::*;$(pub unsafe extern "C" fn $op(_id:Oop,c:Oop,r:Oop,a:*const Oop,n:usize)->Oop{
        let s=match unsafe{state(c)}{Ok(s)=>s,Err(e) => return abi::fault_reference(e)};
        let Some(a)=(unsafe{abi::borrowed_slice(a,n)})else{return s.id.invalid;};
        unsafe{s.perform(Op::$op,r,a)}.unwrap_or_else(abi::fault_reference)
    })*}
    static METHODS:&[MethodSpec]=&[$(MethodSpec{name:$name,entry:Some(entries::$op)}),*,
        MethodSpec{name:PRINT_BOOLEAN,entry:Some(print)}];
};}
macro_rules! literals {
    ($($key:ident => $name:literal),* $(,)?) => {
        const NAMES: &[&[u8]] = &[$($name),*];
        static LITERALS: &[Literal] = &abi::id_protocol::literal_table::<{ NAMES.len() }>(NAMES);
        $(const $key: usize = abi::id_protocol::literal_index(NAMES, $name);)*
    };
}
literals![
    AT => b"at:",
    PUT => b"at:put:",
    ROOTS => b"Roots",
    OBJECT_BEHAVIOR => b"objectBehavior",
    DELEGATED => b"delegated",
    ALLOCATE => b"allocate",
    METHOD_AT => b"methodAt:",
    WITH_CONTEXT => b"withContext:",
    ADD_METHOD => b"addMethod",
    RETAIN => b"retain:",
    RELEASE => b"release:",
    REGISTER => b"register:",
    ROOTS_DO => b"rootsDo:",
    VISIT => b"visit:",
    IDENTICAL => b"==",
    NOT => b"not",
    AND => b"&",
    OR => b"|",
    IF_BOTH => b"ifTrue:ifFalse:",
    IF_TRUE => b"ifTrue:",
    IF_FALSE => b"ifFalse:",
    AND_BLOCK => b"and:",
    OR_BLOCK => b"or:",
    INSTALL_PRINTING => b"installPrinting:",
    BOOLEAN => b"Boolean",
    FALSE_PROTOTYPE => b"False",
    TRUE => b"true",
    FALSE => b"false",
    NIL => b"nil",
    VALUE => b"value",
    BEHAVIOR_OF => b"behaviorOf:",
    LOOKUP => b"lookup",
    PRINT_STRING => b"printString",
    PRINT_BOOLEAN => b"printBoolean",
    LITERAL_AT => b"literalAt:",
    FROM_BYTES => b"fromBytes:",
];
operations![
    (IDENTICAL, Identical),
    (NOT, Not),
    (AND, And),
    (OR, Or),
    (IF_BOTH, IfBoth),
    (IF_TRUE, IfTrue),
    (IF_FALSE, IfFalse),
    (AND_BLOCK, AndBlock),
    (OR_BLOCK, OrBlock),
    (INSTALL_PRINTING, InstallPrinting),
    (ROOTS_DO, Roots)
];
unsafe extern "C" fn print(_id: Oop, c: Oop, r: Oop, _a: *const Oop, n: usize) -> Oop {
    let raw = INSTANCE.load(Ordering::Acquire);
    if raw.is_null() {
        std::process::abort();
    }
    let s = unsafe { &*raw };
    if n != 0 {
        return s.id.invalid;
    }
    (|| unsafe {
        // The only per-Method state is its String dependency: use that Oop
        // directly as context, with no wrapper or per-field message reads.
        let text = match s.polarity(r) {
            Some(true) => s.true_text,
            Some(false) => s.false_text,
            None => return Ok(s.nil),
        };
        s.send(c, FROM_BYTES, &[text])
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    argc: usize,
) -> Oop {
    API.store(p.cast_mut(), Ordering::Release);
    let id = unsafe { *p };
    let Some([namespace]) = (unsafe { abi::borrowed_slice(a, argc) }) else {
        return id.invalid;
    };
    let Some(symbols) = (unsafe { abi::borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if symbols.len() != NAMES.len() {
        return id.invalid;
    }
    let old = INSTANCE.load(Ordering::Acquire);
    if !old.is_null() {
        let s = unsafe { &*old };
        return if s.ready.get() && s.namespace == *namespace {
            s.truth
        } else {
            id.invalid
        };
    }
    (|| unsafe {
        let send = |r, selector: usize, a: &[Oop]| id.send(r, symbols[selector], a);
        let roots = send(*namespace, AT, &[symbols[ROOTS]])?;
        let parent = send(id.root, OBJECT_BEHAVIOR, &[])?;
        let mut tokens = Vec::new();
        let result = (|| {
            let mut keep = |v| -> Result<Oop, Oop> {
                tokens.push(send(roots, RETAIN, &[v])?);
                Ok(v)
            };
            let nil_behavior = keep(send(parent, DELEGATED, &[])?)?;
            let behavior = keep(send(parent, DELEGATED, &[])?)?;
            let nil = keep(send(nil_behavior, ALLOCATE, &[abi::size(0).unwrap()])?)?;
            let truth = keep(send(behavior, ALLOCATE, &[abi::size(0).unwrap()])?)?;
            let falsity = keep(send(behavior, ALLOCATE, &[abi::size(0).unwrap()])?)?;
            let true_text = send(image, LITERAL_AT, &[abi::size(TRUE).unwrap()])?;
            let false_text = send(image, LITERAL_AT, &[abi::size(FALSE).unwrap()])?;
            let s = Box::into_raw(Box::new(State {
                id,
                namespace: *namespace,
                roots,
                image,
                nil,
                truth,
                falsity,
                names: symbols.try_into().unwrap(),
                true_text,
                false_text,
                printing: Cell::new(false),
                ready: Cell::new(false),
            }));
            INSTANCE.store(s, Ordering::Release);
            let s = &*s;
            for spec in METHODS {
                if spec.name == PRINT_BOOLEAN {
                    continue;
                }
                let method = s.send(image, METHOD_AT, &[symbols[spec.name]])?;
                let method = s.send(method, WITH_CONTEXT, &[truth])?;
                let target = if spec.name == IDENTICAL {
                    parent
                } else {
                    behavior
                };
                s.send(target, ADD_METHOD, &[symbols[spec.name], method])?;
            }
            s.send(roots, REGISTER, &[truth])?;
            for (name, value) in [
                (BOOLEAN, truth),
                (FALSE_PROTOTYPE, falsity),
                (TRUE, truth),
                (FALSE, falsity),
                (NIL, nil),
            ] {
                s.send(*namespace, PUT, &[s.names[name], value])?;
            }
            s.ready.set(true);
            Ok(truth)
        })();
        for token in tokens {
            send(roots, RELEASE, &[token])?;
        }
        result
    })()
    .unwrap_or_else(abi::fault_reference)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v2: Module = Module {
    version: 2,
    byte_size: size_of::<Module>() as u32,
    literals: LITERALS.as_ptr(),
    literal_count: LITERALS.len(),
    methods: METHODS.as_ptr(),
    method_count: METHODS.len(),
    initialize: Some(initialize),
};
